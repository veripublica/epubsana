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

use std::collections::BTreeSet;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use epubsana::envelope;
use epubsana::{ChangeReport, Confirmer, Decision, Goal, Outcome, Policy, ProposedFix, repair};
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
        --apply <LIST>      Apply exactly the listed fixes and skip the rest;
                            asks nothing. A selector is a 1-based index from a
                            preceding --dry-run, or a fix id to take every
                            proposal from that fixer. Comma-separated, e.g.
                            --apply 1,4,fix.html_entities. A selector that
                            matches nothing is an error and writes no file.
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
    epubsana -i book.epub --dry-run --format json   # plan, with an index per fix
    epubsana -i book.epub --apply 1,3               # apply just those two

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
    apply: Option<Vec<String>>,
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
    let mut apply: Option<String> = None;
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
                "input" | "output" | "format" | "goal" | "apply" => {
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
                        "apply" => set_single!(apply, "--apply", value),
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
    if let Some(f) = &format
        && !["human", "json"].contains(&f.as_str())
    {
        fail!("invalid value '{f}' for --format; supported values: human, json");
    }
    if let Some(g) = &goal
        && !["valid", "openable"].contains(&g.as_str())
    {
        fail!("invalid value '{g}' for --goal; supported values: valid, openable");
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
            apply: apply.as_deref().map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .map(str::to_string)
                    .collect()
            }),
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

    // --apply states every decision up front, so it contradicts anything that
    // would decide differently. Refuse rather than pick a winner: a plugin that
    // passes both has a bug, and silently honouring one of them applies a set of
    // edits nobody asked for — irreversible, on someone's book.
    if let Some(sel) = &run.apply {
        for (flag, on) in [
            ("--dry-run", run.dry_run),
            ("--yes", run.yes),
            ("--auto-safe", run.auto_safe),
        ] {
            if on {
                return Err(format!(
                    "--apply and {flag} contradict each other: --apply already \
                     answers every prompt, and only for the fixes you listed"
                ));
            }
        }
        if sel.is_empty() {
            return Err(
                "--apply was given no selectors; to apply nothing, simply do not run \
                 the repair, and to apply everything use --yes"
                    .to_string(),
            );
        }
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
    let interactive = !run.yes && run.apply.is_none() && policy != Policy::DryRun;
    if interactive && !io::stdin().is_terminal() {
        return Err(
            "stdin is not a terminal, so epubsana cannot ask about each fix; \
             re-run with --yes to approve every proposed fix, or --dry-run to see them"
                .to_string(),
        );
    }

    let mut confirmer: Box<dyn Confirmer> = if let Some(sel) = &run.apply {
        Box::new(SelectionConfirmer::new(sel))
    } else if run.yes {
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

    // A selector that matched nothing means the caller was describing a plan we
    // did not produce — a stale index from an older dry run, a typo, a fixer that
    // proposed nothing this time. Refuse the whole run rather than apply the
    // subset that did match: a plugin asking for fixes 1, 3 and 7 and silently
    // getting two of them has been told the wrong thing about someone's book.
    //
    // This runs *after* repair and *before* the write, which is the only reason
    // it is safe: the workspace is mutated in memory, and nothing has touched the
    // disk yet, so returning here leaves the input exactly as it was.
    if let Some(sel) = &run.apply {
        let unmatched: Vec<&str> = sel
            .iter()
            .filter(|s| !match s.parse::<usize>() {
                Ok(n) => n >= 1 && n <= report.fixes.len(),
                Err(_) => report.fixes.iter().any(|f| f.fix_id == s.as_str()),
            })
            .map(String::as_str)
            .collect();
        if !unmatched.is_empty() {
            return Err(format!(
                "--apply selector(s) matched no proposed fix: {}. This run planned {} \
                 fix(es); re-run with --dry-run to see the current plan. Nothing was \
                 written.",
                unmatched.join(", "),
                report.fixes.len()
            ));
        }
    }

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
    for (i, f) in report.fixes.iter().enumerate() {
        println!(
            "[{}] {} {}",
            i + 1,
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

/// Approves exactly the fixes `--apply` named, and rejects the rest.
///
/// The [`Confirmer`] trait is the whole extension point here — "confirm each
/// step" lives in the core as a question, so answering it from a list instead of
/// a terminal needs no change to the repair pipeline at all.
///
/// A selector that parses as a number is **always** an index and never a fixer
/// name, which is the same split the post-run validation uses — if the two ever
/// disagreed, a selector could validate as one kind and select as the other.
///
/// Two kinds of selector, deliberately: a **1-based index** into the plan
/// (what a plugin round-trips from a `--dry-run` envelope, and what the human
/// report now prints in brackets), and a **fix id** like `fix.html_entities`,
/// which takes every proposal from that fixer. The first is for "the user picked
/// these three"; the second is for "I trust this repair and not that one".
///
/// Indices are only meaningful because planning is deterministic — same input
/// and same detector version give the same plan in the same order. That is a
/// promise this flag now depends on, not an implementation detail.
struct SelectionConfirmer {
    selectors: BTreeSet<String>,
    seen: usize,
}

impl SelectionConfirmer {
    fn new(selectors: &[String]) -> Self {
        SelectionConfirmer {
            selectors: selectors.iter().cloned().collect(),
            seen: 0,
        }
    }
}

impl SelectionConfirmer {
    /// Does the plan's `index`-th fix, produced by `fix_id`, match the list?
    ///
    /// Split out from [`Confirmer::decide`] so the selection rule can be tested
    /// on its own: `ProposedFix` carries a boxed closure and cannot be built
    /// outside the crate, and the rule is the part worth pinning anyway.
    fn selects(&self, index: usize, fix_id: &str) -> bool {
        self.selectors.iter().any(|s| match s.parse::<usize>() {
            Ok(n) => n == index,
            Err(_) => s == fix_id,
        })
    }
}

impl Confirmer for SelectionConfirmer {
    fn decide(&mut self, fix: &ProposedFix) -> Decision {
        // Counts every proposal, not every approval: the index has to keep
        // meaning "position in the plan" no matter what was rejected before it,
        // because that is what the dry run showed the caller.
        self.seen += 1;
        if self.selects(self.seen, fix.fix_id) {
            Decision::Approve
        } else {
            Decision::Reject
        }
    }
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
    fn apply_splits_and_trims_its_selector_list() {
        let run = run_of(&["-i", "a.epub", "--apply", "1, 4 ,fix.html_entities"]);
        assert_eq!(
            run.apply.unwrap(),
            vec!["1", "4", "fix.html_entities"],
            "whitespace around a comma is a human typing a list, not a selector"
        );
    }

    #[test]
    fn apply_drops_empty_selectors_rather_than_matching_nothing() {
        // A trailing comma is the shape a generated list arrives in.
        let run = run_of(&["-i", "a.epub", "--apply", "2,,"]);
        assert_eq!(run.apply.unwrap(), vec!["2"]);
    }

    #[test]
    fn apply_is_absent_unless_asked_for() {
        assert!(run_of(&["-i", "a.epub"]).apply.is_none());
    }

    #[test]
    fn selection_takes_an_index_and_leaves_its_neighbours() {
        let c = SelectionConfirmer::new(&["2".to_string()]);
        assert!(!c.selects(1, "fix.alpha"));
        assert!(c.selects(2, "fix.beta"));
        assert!(!c.selects(3, "fix.alpha"));
    }

    #[test]
    fn selection_takes_every_proposal_of_a_named_fixer() {
        // One fixer can propose several times in a plan (one per file); naming
        // it must take all of them, whatever position they sit at.
        let c = SelectionConfirmer::new(&["fix.alpha".to_string()]);
        assert!(c.selects(1, "fix.alpha"));
        assert!(!c.selects(2, "fix.beta"));
        assert!(c.selects(9, "fix.alpha"));
    }

    #[test]
    fn selection_mixes_indices_and_fixer_ids() {
        let c = SelectionConfirmer::new(&["3".to_string(), "fix.alpha".to_string()]);
        assert!(c.selects(1, "fix.alpha"));
        assert!(c.selects(3, "fix.beta"));
        assert!(!c.selects(2, "fix.beta"));
    }

    #[test]
    fn selection_does_not_confuse_an_index_with_a_fixer_id() {
        // A fixer is never named by a bare number, so "1" can only be a position.
        let c = SelectionConfirmer::new(&["1".to_string()]);
        assert!(!c.selects(2, "1"));
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
