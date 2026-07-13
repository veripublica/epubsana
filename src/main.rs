//! epubsana's CLI, following the **veripublica CLI convention v0.4**
//! (<https://github.com/veripublica/conventions>).
//!
//! epubsana is a *transformer*: it takes **exactly one** input, writes a
//! repaired copy beside it, and asks before every change. So, unlike epubveri,
//! the whole of the convention applies here — the output-safety rules (`-o`,
//! `-f`, never in place, never a silent overwrite) and the prompt rules (`-y`,
//! and never a prompt when stdin is not a TTY) included.
//!
//! The argument grammar is epubveri's, ported deliberately rather than
//! re-derived: one family, one parser, one set of surprises.
//!
//! Exit codes: `0` = the run's goal was met, `1` = it was not, `2` = the tool
//! could not run.

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use epubsana::envelope;
use epubsana::{repair, ChangeReport, Confirmer, Decision, Goal, Outcome, Policy, ProposedFix};
use epubsana::{Tier, Workspace};

const HELP: &str = "\
epubsana — repair the EPUB defects epubveri detects

USAGE:
    epubsana -i <PATH> [OPTIONS]

OPTIONS:
    -i, --input <PATH>      The input. The only input form; positional paths are
                            not accepted.
    -o, --output <PATH>     Where to write the output. Defaults to
                            <input-stem>_fixed.epub, beside the input.
    -f, --force             Permit replacing existing output files. Never lifts
                            the output-equals-input refusal.
        --format <FORMAT>   Report format: human (the default) or json. json is
                            the shared machine envelope (one JSON object, see the
                            veripublica FORMATS spec).
        --dry-run           Report what would happen; change nothing on disk.
    -y, --yes               Assume \"yes\" for every prompt; run non-interactively.
                            Not permission to overwrite files — that is -f.
        --auto-safe         Apply the provably-safe fixes without asking; still
                            prompt for the ones that need a decision.
        --goal <GOAL>       How far to repair: valid (the default) or openable.
                            See EXIT CODES.
    -v, --verbose           Emit more detail: each fix's rationale (why it is safe).
    -V, --version           Print epubsana <version> to stdout and exit 0.
    -h, --help              Print this help to stdout and exit 0.

EXAMPLES:
    epubsana -i book.epub --dry-run        # preview the fixes; change nothing
    epubsana -i book.epub                  # repair, approving each fix
    epubsana -i book.epub --auto-safe      # apply the safe ones; ask about the rest
    epubsana -i book.epub -y -o fixed.epub # no prompts, explicit output path
    epubsana -i book.epub --format json -y # the machine envelope on stdout

The original is never modified in place: repairs go to a separate file, and an
existing output file is never silently replaced (use -f).

EXIT CODES:
    0   the run's goal was met.
          --goal valid    (default) no fatal- and no error-severity findings
                          remain — the book is valid.
          --goal openable no fatal-severity findings remain — the book opens.
                          Errors may remain, and are still reported: the exit
                          code answers the question the invocation asked.
    1   the goal was not met: fixes were declined, or defects epubsana cannot
        fix remain.
    2   epubsana could not run: a usage error, an unreadable EPUB, an output
        path that is the input, an existing output file without -f, an
        unanswerable prompt, or an I/O failure.

Conforms to veripublica conventions v0.4.";

/// The outcome of parsing `argv` — decided entirely before any work is done.
#[derive(Debug, PartialEq)]
enum Cli {
    Run(Run),
    /// `-h`/`--help` was requested (short-circuits everything else).
    Help,
    /// `-V`/`--version` was requested.
    Version,
    /// The invocation was malformed; the string is the short problem message
    /// (without the `error:` prefix or the `--help` pointer main adds).
    Usage(String),
}

#[derive(Debug, PartialEq)]
struct Run {
    input: String,
    output: Option<String>,
    force: bool,
    format: String,
    dry_run: bool,
    yes: bool,
    auto_safe: bool,
    goal: Goal,
    verbose: bool,
}

/// Parse the arguments after the program name into a [`Cli`] decision.
///
/// The accepted syntaxes are the convention's (§3.3): `--name value` and
/// `--name=value`; `-i value` and the attached `-ivalue`; boolean short flags
/// bundle (`-yv`); a value-taking short flag consumes the rest of its token, or
/// the next token, as its value (POSIX: `-iv` means `-i v`); and the token after
/// a value-taking option is *always* its value, never re-parsed as an option
/// (`-i -q.epub` names the file `-q.epub`).
fn parse(args: &[String]) -> Cli {
    let mut inputs: Vec<String> = Vec::new();
    let mut output: Option<String> = None;
    let mut format: Option<String> = None;
    let mut goal: Option<String> = None;
    let mut force = false;
    let mut dry_run = false;
    let mut yes = false;
    let mut auto_safe = false;
    let mut verbose = false;
    let mut help = false;
    let mut version = false;
    let mut error: Option<String> = None;

    // Record the first usage error but keep scanning, so a later `-h` can still
    // short-circuit a malformed line (§5). Help wins over any error below.
    macro_rules! fail {
        ($($a:tt)*) => {{ if error.is_none() { error = Some(format!($($a)*)); } }};
    }
    // Assign a value to a single-valued option, rejecting a second answer (§3.4).
    macro_rules! set_single {
        ($slot:expr, $name:literal, $value:expr) => {{
            if $slot.is_some() {
                fail!(concat!("option '", $name, "' given more than once"));
            } else {
                $slot = Some($value);
            }
        }};
    }

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            // Accepted and ignored; the convention gives it no other meaning.
        } else if let Some(long) = arg.strip_prefix("--") {
            let (name, attached) = match long.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (long, None),
            };
            match name {
                "help" => help = true,
                "version" => version = true,
                "force" => force = true,
                "dry-run" => dry_run = true,
                "yes" => yes = true,
                "auto-safe" => auto_safe = true,
                "verbose" => verbose = true,
                "input" | "output" | "format" | "goal" => {
                    let value = match attached {
                        Some(v) => v,
                        None => {
                            i += 1;
                            match args.get(i) {
                                Some(v) => v.clone(),
                                None => {
                                    fail!("option '--{name}' needs a value");
                                    break;
                                }
                            }
                        }
                    };
                    match name {
                        "input" => inputs.push(value),
                        "output" => set_single!(output, "--output", value),
                        "format" => set_single!(format, "--format", value),
                        "goal" => set_single!(goal, "--goal", value),
                        _ => unreachable!(),
                    }
                }
                _ => fail!("unexpected option '--{name}'"),
            }
        } else if arg.len() > 1 && arg.starts_with('-') {
            // A short cluster: booleans bundle; the first value-taking flag ends
            // it by consuming the remainder of the token (or the next token).
            let chars: Vec<char> = arg[1..].chars().collect();
            let mut j = 0;
            while j < chars.len() {
                match chars[j] {
                    'h' => help = true,
                    'V' => version = true,
                    'f' => force = true,
                    'y' => yes = true,
                    'v' => verbose = true,
                    c @ ('i' | 'o') => {
                        let rest: String = chars[j + 1..].iter().collect();
                        let value = if !rest.is_empty() {
                            rest
                        } else {
                            i += 1;
                            match args.get(i) {
                                Some(v) => v.clone(),
                                None => {
                                    fail!("option '-{c}' needs a value");
                                    break;
                                }
                            }
                        };
                        match c {
                            'i' => inputs.push(value),
                            _ => set_single!(output, "--output", value),
                        }
                        break; // the value-taking flag consumed the rest of the cluster
                    }
                    c => {
                        fail!("unexpected option '-{c}'");
                        break;
                    }
                }
                j += 1;
            }
        } else {
            // A bare word: positional inputs are not accepted (§2). Point the
            // user straight at the form that works.
            fail!("unexpected argument '{arg}'; use -i {arg}");
        }
        i += 1;
    }

    // Reject an out-of-set value for an enum option (§3.5) — after the scan, so
    // a `-h` anywhere still short-circuits to help rather than this error.
    if let Some(f) = &format {
        if !["human", "json"].contains(&f.as_str()) {
            fail!("invalid value '{f}' for --format; supported values: human, json");
        }
    }
    if let Some(g) = &goal {
        if !["valid", "openable"].contains(&g.as_str()) {
            fail!("invalid value '{g}' for --goal; supported values: valid, openable");
        }
    }

    // Precedence: help short-circuits even a malformed line; a usage error
    // outranks a version request; version outranks a run; a run needs an input.
    if help {
        return Cli::Help;
    }
    if let Some(msg) = error {
        return Cli::Usage(msg);
    }
    if version {
        return Cli::Version;
    }
    // A transformer takes exactly one input (§2): a second `-i` is a usage
    // error, never a silently-kept last one.
    match inputs.len() {
        0 => Cli::Usage("missing required -i".to_string()),
        1 => Cli::Run(Run {
            input: inputs.remove(0),
            output,
            force,
            format: format.unwrap_or_else(|| "human".to_string()),
            dry_run,
            yes,
            auto_safe,
            goal: match goal.as_deref() {
                Some("openable") => Goal::Openable,
                _ => Goal::Valid,
            },
            verbose,
        }),
        n => Cli::Usage(format!(
            "epubsana repairs one book at a time: expected 1 input, got {n}"
        )),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Cli::Help => {
            println!("{HELP}");
            ExitCode::SUCCESS
        }
        Cli::Version => {
            println!("epubsana {}", epubsana::VERSION);
            ExitCode::SUCCESS
        }
        Cli::Usage(msg) => {
            // Short stderr message + a pointer to --help; never the full help.
            eprintln!("error: {msg} (see --help)");
            ExitCode::from(2)
        }
        Cli::Run(run) => match execute(&run) {
            Ok(code) => code,
            Err(e) => {
                // A failure that leaves no verdict: no envelope, even in json
                // mode (FORMATS.md §1 — the envelope describes runs that
                // happened). One input, so there is nothing else to report on.
                eprintln!("error: {e}");
                ExitCode::from(2)
            }
        },
    }
}

/// Repair the one input, report, and return the exit code: `0` if the run's goal
/// was met, else `1`. Anything that stops the run from producing a verdict is an
/// `Err` — exit `2`.
fn execute(run: &Run) -> Result<ExitCode, String> {
    let input = Path::new(&run.input);
    let json = run.format == "json";

    let bytes =
        std::fs::read(input).map_err(|e| format!("cannot read {}: {e}", input.display()))?;
    let mut ws = Workspace::load(&bytes).map_err(|e| format!("cannot read {}: {e}", run.input))?;

    // Resolve the output and enforce the file-safety rules *before* any work —
    // including under --dry-run, so `epubsana --dry-run … && epubsana …` never
    // surprises on the second half (§3.7).
    let out = match &run.output {
        Some(o) => PathBuf::from(o),
        None => default_output(input),
    };
    if same_path(input, &out) {
        return Err(format!(
            "output path is the input ({}); refusing to modify the original in place \
             — choose a different -o (-f does not lift this)",
            out.display()
        ));
    }
    if out.exists() && !run.force {
        return Err(format!("'{}' exists; use -f to replace it", out.display()));
    }

    let policy = if run.dry_run {
        Policy::DryRun
    } else if run.auto_safe {
        Policy::AutoSafeThenAsk
    } else {
        Policy::AskEach
    };

    // A prompt we cannot ask is a decision we cannot obtain: stop loudly rather
    // than silently answering "no" and returning an exit code that looks like an
    // ordinary result (§5). --yes and --dry-run ask nothing; --auto-safe still
    // asks about the fixes that need a decision.
    let interactive = !run.yes && policy != Policy::DryRun;
    if interactive && !io::stdin().is_terminal() {
        return Err(
            "stdin is not a terminal, so epubsana cannot ask about each fix; \
             re-run with --yes to approve every proposed fix, or --dry-run to see them"
                .to_string(),
        );
    }

    let mut confirmer: Box<dyn Confirmer> = if run.yes {
        Box::new(YesConfirmer)
    } else {
        Box::new(TtyConfirmer {
            verbose: run.verbose,
            json,
        })
    };

    // In json mode the human progress line would land on stdout and break the
    // "exactly one JSON object" guarantee — so it simply isn't printed.
    if !json {
        let before = ws.detect().map_err(|e| e.to_string())?;
        println!(
            "{}: {} before repair",
            run.input,
            counts(before.fatals(), before.errors(), before.warnings())
        );
    }

    let report =
        repair(&mut ws, run.goal, policy, confirmer.as_mut()).map_err(|e| e.to_string())?;

    // Write only when something was actually applied — a run whose every fix was
    // declined has nothing to write, and leaves no file behind to explain. Under
    // --dry-run nothing is written at all; `output` then names the path that
    // *would* be, and only when there is something to write there.
    let written = if run.dry_run {
        (!report.fixes.is_empty()).then(|| out.display().to_string())
    } else if report.changed() {
        std::fs::write(&out, ws.serialize().map_err(|e| e.to_string())?)
            .map_err(|e| format!("cannot write {}: {e}", out.display()))?;
        Some(out.display().to_string())
    } else {
        None
    };

    if json {
        let input = envelope::input(run.input.clone(), written, &report);
        let env = envelope::envelope(input, run.dry_run);
        println!("{}", serde_json::to_string_pretty(&env).unwrap());
    } else {
        print_report(&report, written.as_deref(), run);
    }

    Ok(if report.goal_met {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

/// The human report: what was proposed, what became of it, and the verdict.
fn print_report(report: &ChangeReport, written: Option<&str>, run: &Run) {
    println!(
        "\n— {} —",
        if run.dry_run {
            "proposed fixes (dry run: nothing was changed)"
        } else {
            "repair report"
        }
    );
    if report.fixes.is_empty() {
        println!("No fixes to propose.");
    }
    for f in &report.fixes {
        println!(
            "{} {}",
            match f.outcome {
                Outcome::Applied => "APPLIED",
                Outcome::Skipped => "SKIPPED",
                Outcome::Proposed => "WOULD APPLY",
            },
            f.title
        );
        if run.verbose {
            println!("    why: {}", f.rationale);
        }
        for c in &f.changes {
            println!("    - {}", c.note);
        }
    }

    println!(
        "\n{} → {}",
        counts(report.fatals_before, report.errors_before, 0),
        counts(report.fatals_after, report.errors_after, 0),
    );
    if let Some(path) = written {
        println!(
            "{} {path}",
            if run.dry_run { "would write" } else { "wrote" }
        );
    }
    println!(
        "goal '{}': {}",
        report.goal.as_str(),
        if report.goal_met { "MET" } else { "NOT MET" }
    );
}

/// "N fatal(s), N error(s), N warning(s)" — fatals first and always, because a
/// fatal-only book has zero errors and is not remotely valid.
fn counts(fatals: usize, errors: usize, warnings: usize) -> String {
    let mut s = format!("{fatals} fatal(s), {errors} error(s)");
    if warnings > 0 {
        s.push_str(&format!(", {warnings} warning(s)"));
    }
    s
}

/// Default output: `<input-stem>_fixed.epub`, beside the input (§4).
fn default_output(input: &Path) -> PathBuf {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("book");
    input.with_file_name(format!("{stem}_fixed.epub"))
}

/// Whether `output` resolves to the same file as `input` (so we never overwrite
/// the original). Handles the output not existing yet by resolving its parent.
fn same_path(input: &Path, output: &Path) -> bool {
    let Ok(ci) = std::fs::canonicalize(input) else {
        return false;
    };
    if let Ok(co) = std::fs::canonicalize(output) {
        return ci == co;
    }
    match (output.parent(), output.file_name()) {
        (Some(parent), Some(name)) => {
            let parent = if parent.as_os_str().is_empty() {
                Path::new(".")
            } else {
                parent
            };
            std::fs::canonicalize(parent)
                .map(|cp| cp.join(name) == ci)
                .unwrap_or(false)
        }
        _ => false,
    }
}

/// Render a proposed fix and its preview, for the prompt.
fn format_fix(fix: &ProposedFix, verbose: bool) -> String {
    let tier = match fix.tier {
        Tier::AutoSafe => "safe",
        Tier::ConfirmNeeded => "needs a decision",
    };
    let mut lines = vec![format!("[{tier}] {}", fix.title)];
    if verbose {
        lines.push(format!("    why: {}", fix.rationale));
    }
    for c in &fix.preview {
        lines.push(format!("    - {}", c.note));
    }
    lines.join("\n")
}

/// Approves every fix (for `--yes`).
struct YesConfirmer;
impl Confirmer for YesConfirmer {
    fn decide(&mut self, _fix: &ProposedFix) -> Decision {
        Decision::Approve
    }
}

/// Prompts on the terminal for each fix. Prompts go to stderr so stdout carries
/// only the report — and, in json mode, only the one JSON object (§5).
struct TtyConfirmer {
    verbose: bool,
    json: bool,
}
impl Confirmer for TtyConfirmer {
    fn decide(&mut self, fix: &ProposedFix) -> Decision {
        eprintln!("\n{}", format_fix(fix, self.verbose || self.json));
        eprint!("  Apply this fix? [y/N] ");
        io::stderr().flush().ok();
        let mut line = String::new();
        if io::stdin().read_line(&mut line).is_ok() && line.trim().eq_ignore_ascii_case("y") {
            Decision::Approve
        } else {
            Decision::Reject
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(argv: &[&str]) -> Cli {
        parse(&argv.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    fn run_of(argv: &[&str]) -> Run {
        match parse_str(argv) {
            Cli::Run(run) => run,
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn bare_invocation_is_missing_input_not_help() {
        assert_eq!(parse_str(&[]), Cli::Usage("missing required -i".into()));
    }

    #[test]
    fn positional_is_rejected_with_a_migration_hint() {
        assert_eq!(
            parse_str(&["book.epub"]),
            Cli::Usage("unexpected argument 'book.epub'; use -i book.epub".into())
        );
    }

    #[test]
    fn input_forms_all_name_the_same_file() {
        for argv in [
            vec!["-i", "book.epub"],
            vec!["--input", "book.epub"],
            vec!["--input=book.epub"],
            vec!["-ibook.epub"],
        ] {
            let run = run_of(&argv);
            assert_eq!(run.input, "book.epub");
            assert_eq!(run.format, "human");
            assert_eq!(run.goal, Goal::Valid);
        }
    }

    #[test]
    fn a_second_input_is_a_usage_error_not_a_silent_last_wins() {
        assert_eq!(
            parse_str(&["-i", "a.epub", "-i", "b.epub"]),
            Cli::Usage("epubsana repairs one book at a time: expected 1 input, got 2".into())
        );
    }

    #[test]
    fn a_value_token_is_never_reparsed_as_an_option() {
        assert_eq!(run_of(&["-i", "-q.epub"]).input, "-q.epub");
    }

    #[test]
    fn bundled_value_flag_takes_the_remainder_posix() {
        // -iv means -i v, not -i -v.
        assert_eq!(run_of(&["-iv"]).input, "v");
    }

    #[test]
    fn boolean_shorts_bundle() {
        let run = run_of(&["-yfv", "-i", "a.epub"]);
        assert!(run.yes && run.force && run.verbose);
    }

    #[test]
    fn repeated_single_valued_option_is_an_error() {
        assert_eq!(
            parse_str(&["-i", "a.epub", "--format", "human", "--format", "json"]),
            Cli::Usage("option '--format' given more than once".into())
        );
        assert_eq!(
            parse_str(&["-i", "a.epub", "-o", "x.epub", "-o", "y.epub"]),
            Cli::Usage("option '--output' given more than once".into())
        );
    }

    #[test]
    fn repeated_boolean_is_not_an_error() {
        assert!(run_of(&["-i", "a.epub", "-v", "--verbose", "-v"]).verbose);
    }

    #[test]
    fn unknown_option_is_a_usage_error() {
        assert_eq!(
            parse_str(&["-x", "-i", "a.epub"]),
            Cli::Usage("unexpected option '-x'".into())
        );
        assert_eq!(
            parse_str(&["--bogus"]),
            Cli::Usage("unexpected option '--bogus'".into())
        );
    }

    #[test]
    fn unknown_enum_values_are_rejected_with_the_supported_set() {
        assert_eq!(
            parse_str(&["-i", "a.epub", "--format", "xml"]),
            Cli::Usage("invalid value 'xml' for --format; supported values: human, json".into())
        );
        assert_eq!(
            parse_str(&["-i", "a.epub", "--goal", "perfect"]),
            Cli::Usage(
                "invalid value 'perfect' for --goal; supported values: valid, openable".into()
            )
        );
    }

    #[test]
    fn goal_and_format_pass_through_when_valid() {
        let run = run_of(&["--goal", "openable", "--format=json", "-i", "a.epub"]);
        assert_eq!(run.goal, Goal::Openable);
        assert_eq!(run.format, "json");
    }

    #[test]
    fn help_short_circuits_even_a_malformed_line() {
        assert_eq!(parse_str(&["--bogus", "-h"]), Cli::Help);
        assert_eq!(parse_str(&["-h"]), Cli::Help);
        // Help wins over version, and over a bundle carrying both.
        assert_eq!(parse_str(&["-hV"]), Cli::Help);
    }

    #[test]
    fn version_is_recognized_and_needs_no_input() {
        assert_eq!(parse_str(&["-V"]), Cli::Version);
        assert_eq!(parse_str(&["--version"]), Cli::Version);
    }

    #[test]
    fn default_output_sits_beside_the_input() {
        assert_eq!(
            default_output(Path::new("/books/Aylak Adam.epub")),
            PathBuf::from("/books/Aylak Adam_fixed.epub")
        );
    }
}
