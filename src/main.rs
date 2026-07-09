//! epubsana CLI — repair an EPUB's defects, confirming each fix and reporting
//! exactly what changed. Conforms to the veripublica CLI convention v1
//! (see the `veripublica/conventions` repository).

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use epubsana::{fixers, repair, Confirmer, Decision, Goal, Policy, ProposedFix, Workspace};

const HELP: &str = "\
epubsana — repair the EPUB defects epubveri detects

USAGE:
    epubsana -i <book.epub> [OPTIONS]
    epubsana <book.epub> [OPTIONS]

OPTIONS:
    -i, --input <path>       the EPUB to repair (a positional path also works)
    -o, --output <path>      write the repaired EPUB here
                             (default: <name>_fixed.epub, next to the input)
    --dry-run                show the fixes that would be applied, change nothing
    --yes                    apply every proposed fix without prompting
    --auto-safe              auto-apply safe fixes, prompt for the rest
    --goal <openable|valid>  how far to repair (default: valid)
    -V, --version            print version and exit
    -h, --help               print this help and exit

The original is never modified in place: repairs go to a separate file, and
epubsana refuses to run if the output path is the input.

EXIT CODES:
    0  the book is valid after repair (or was already)
    1  repair ran, but some errors remain
    2  the tool could not run (bad arguments, unreadable EPUB, output == input, I/O)

Conforms to the veripublica CLI convention v1.
";

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut dry_run = false;
    let mut yes = false;
    let mut auto_safe = false;
    let mut goal = Goal::Valid;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(ExitCode::SUCCESS);
            }
            "-V" | "--version" => {
                println!("epubsana {}", env!("CARGO_PKG_VERSION"));
                return Ok(ExitCode::SUCCESS);
            }
            "-i" | "--input" => {
                input = Some(PathBuf::from(args.next().ok_or("--input needs a path")?))
            }
            "-o" | "--output" => {
                output = Some(PathBuf::from(args.next().ok_or("--output needs a path")?))
            }
            "--dry-run" => dry_run = true,
            "--yes" => yes = true,
            "--auto-safe" => auto_safe = true,
            "--goal" => {
                goal = match args.next().as_deref() {
                    Some("openable") => Goal::Openable,
                    Some("valid") | None => Goal::Valid,
                    Some(other) => return Err(format!("unknown --goal '{other}'").into()),
                }
            }
            other if !other.starts_with('-') => input = Some(PathBuf::from(other)),
            other => return Err(format!("unknown option '{other}'").into()),
        }
    }

    let input = input.ok_or("no input EPUB given (use -i <path>, or --help)")?;
    let bytes = std::fs::read(&input)?;
    let mut ws = Workspace::load(&bytes)?;

    let before = ws.detect()?;
    println!(
        "{}: {} error(s), {} warning(s) before repair",
        input.display(),
        before.errors(),
        before.warnings()
    );

    if dry_run {
        let proposals = fixers::plan(&before, &ws, goal);
        if proposals.is_empty() {
            println!("No fixes to propose.");
        } else {
            println!("\nProposed fixes ({}):", proposals.len());
            for fix in &proposals {
                println!("\n{}", format_fix(fix));
            }
        }
        return Ok(exit_code(before.errors()));
    }

    // Resolve the output up front and refuse to touch the original in place.
    let out = output.unwrap_or_else(|| default_output(&input));
    if same_path(&input, &out) {
        return Err(
            "output path is the input; refusing to modify the original in place \
                    (choose a different -o)"
                .into(),
        );
    }

    let policy = if auto_safe {
        Policy::AutoSafeThenAsk
    } else {
        Policy::AskEach
    };
    let mut confirmer: Box<dyn Confirmer> = if yes {
        Box::new(YesConfirmer)
    } else {
        Box::new(TtyConfirmer)
    };

    let report = repair(&mut ws, goal, policy, confirmer.as_mut())?;

    println!("\n— repair report —");
    if report.applied.is_empty() {
        println!("No fixes applied.");
    } else {
        for a in &report.applied {
            println!("APPLIED {}", a.title);
            for c in &a.changes {
                println!("    - {}", c.note);
            }
        }
    }
    if !report.skipped.is_empty() {
        println!("Skipped {} fix(es).", report.skipped.len());
    }
    println!("errors: {} → {}", report.errors_before, report.errors_after);

    if !report.applied.is_empty() {
        std::fs::write(&out, ws.serialize()?)?;
        println!("wrote {}", out.display());
    }

    Ok(exit_code(report.errors_after))
}

/// Convention exit code: `0` = clean, `1` = problems remain.
fn exit_code(errors: usize) -> ExitCode {
    if errors == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// Default output: `<name>_fixed.epub`, next to the input.
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

/// Render a proposed fix and its preview (shared by `--dry-run` and prompts).
fn format_fix(fix: &ProposedFix) -> String {
    let mut lines = vec![format!("[{:?}] {}", fix.tier, fix.title)];
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
/// only the report (per the convention's stream rules).
struct TtyConfirmer;
impl Confirmer for TtyConfirmer {
    fn decide(&mut self, fix: &ProposedFix) -> Decision {
        eprintln!("\n{}", format_fix(fix));
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
