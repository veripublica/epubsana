//! epubsana CLI — repair an EPUB's defects, confirming each fix and reporting
//! exactly what changed.

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use epubsana::{fixers, repair, Confirmer, Decision, Goal, Policy, ProposedFix, Workspace};

const HELP: &str = "\
epubsana — repair the EPUB defects epubveri detects

USAGE:
    epubsana <book.epub> [OPTIONS]

OPTIONS:
    --dry-run              show the fixes that would be applied, change nothing
    --yes                  apply every proposed fix without prompting
    --auto-safe            auto-apply safe fixes, prompt for the rest (default: prompt for all)
    --goal <openable|valid>  how far to repair (default: valid)
    -o, --output <path>    write the repaired EPUB here (default: <name>.fixed.epub)
    -V, --version          print version and exit
    -h, --help             print this help and exit
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
            "-o" | "--output" => {
                output = Some(PathBuf::from(args.next().ok_or("--output needs a path")?))
            }
            other if !other.starts_with('-') => input = Some(PathBuf::from(other)),
            other => return Err(format!("unknown option '{other}'").into()),
        }
    }

    let input = input.ok_or("no input EPUB given (try --help)")?;
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
                print_fix(fix);
            }
        }
        return Ok(ExitCode::SUCCESS);
    }

    let policy = if yes {
        Policy::AskEach // with a confirmer that approves everything
    } else if auto_safe {
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
        let out = output.unwrap_or_else(|| default_output(&input));
        std::fs::write(&out, ws.serialize()?)?;
        println!("wrote {}", out.display());
    }

    Ok(ExitCode::SUCCESS)
}

fn print_fix(fix: &ProposedFix) {
    println!("\n[{:?}] {}", fix.tier, fix.title);
    for c in &fix.preview {
        println!("    - {}", c.note);
    }
}

fn default_output(input: &std::path::Path) -> PathBuf {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("book");
    input.with_file_name(format!("{stem}.fixed.epub"))
}

/// Approves every fix (for `--yes`).
struct YesConfirmer;
impl Confirmer for YesConfirmer {
    fn decide(&mut self, _fix: &ProposedFix) -> Decision {
        Decision::Approve
    }
}

/// Prompts on the terminal for each fix.
struct TtyConfirmer;
impl Confirmer for TtyConfirmer {
    fn decide(&mut self, fix: &ProposedFix) -> Decision {
        print_fix(fix);
        print!("  Apply this fix? [y/N] ");
        io::stdout().flush().ok();
        let mut line = String::new();
        if io::stdin().read_line(&mut line).is_ok() && line.trim().eq_ignore_ascii_case("y") {
            Decision::Approve
        } else {
            Decision::Reject
        }
    }
}
