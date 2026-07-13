//! epubsana's `--format json` — the veripublica machine envelope, built on
//! **epubveri's reference types** ([`epubveri::envelope`], FORMATS.md
//! convention v0.4).
//!
//! The skeleton is not epubsana's: `Envelope`/`Input`/`Item` come from epubveri,
//! generic over the two slots FORMATS.md §2 leaves to each tool — the `summary`
//! aggregate and the per-item `data` extras. This module supplies those two
//! ([`Summary`], [`Data`]) and maps a [`ChangeReport`] into the shape. A
//! consumer that reads epubveri's envelope reads this one with the same parser,
//! and there is exactly one copy of the skeleton in the family.
//!
//! What a *repairer* adds to a verifier's envelope is the per-item `outcome`: a
//! confirm-each-step run routinely applies one fix and declines the next, and a
//! report that cannot say which is not a report of what changed (conventions
//! #25). [`epubveri::envelope::Item::fix`] makes it unconstructible without one.

use serde::Serialize;

use epubveri::envelope::Item;

use crate::{ChangeReport, ReportedFix, Tier};

/// epubsana's envelope, with its two tool-owned slots filled in.
pub type Envelope = epubveri::envelope::Envelope<Summary, Data>;
/// One repaired input, in the shared shape.
pub type Input = epubveri::envelope::Input<Summary, Data>;

/// Build the whole run's envelope: one input (a transformer takes exactly one),
/// `dry_run` set when nothing was written on purpose.
pub fn envelope(input: Input, dry_run: bool) -> Envelope {
    let mut env = Envelope::for_tool("epubsana", crate::VERSION, None, vec![input]);
    env.dry_run = dry_run;
    env
}

/// The repaired input: `ok`/`problems` by whether the run's goal was met, one
/// `fix` item per planned fix, and the path written (or, under a dry run, the
/// path that *would* be written — `None` when there would be nothing to write).
pub fn input(path: String, output: Option<String>, report: &ChangeReport) -> Input {
    Input {
        path,
        status: if report.goal_met { "ok" } else { "problems" },
        error: None,
        output,
        summary: Some(Summary::of(report)),
        items: report.fixes.iter().map(item).collect(),
    }
}

/// An input that could not be read at all: `error`, no verdict.
///
/// epubsana has exactly one input, so in practice a CLI run that cannot read it
/// prints a stderr message and exits `2` with no envelope at all. This exists
/// for an embedder that batches books and still wants one envelope per run.
pub fn input_error(path: String, error: String) -> Input {
    Input {
        path,
        status: "error",
        error: Some(error),
        output: None,
        summary: None,
        items: Vec::new(),
    }
}

/// One planned fix as a `fix` item. `severity` is **inherited** from the finding
/// the fix addresses, verbatim from epubveri — it describes the *defect*, never
/// epubsana's opinion of its own fix.
fn item(f: &ReportedFix) -> Item<Data> {
    Item::fix(
        f.outcome.as_str(),
        f.addresses_id.clone(),
        f.addresses_rule,
        f.addresses_severity.as_str(),
        f.location.clone(),
        None, // a fix spans a file, not a point in it
        f.title.clone(),
        Some(Data {
            fix_id: f.fix_id,
            tier: match f.tier {
                Tier::AutoSafe => "auto_safe",
                Tier::ConfirmNeeded => "confirm_needed",
            },
            changes: f.changes.iter().map(|c| c.note.clone()).collect(),
        }),
    )
}

/// epubsana's `summary` vocabulary (tool-owned; a consumer MUST NOT require it).
///
/// Fatals are counted apart from errors, as epubveri reports them: a book whose
/// defects are all fatal has `errors_before: 0` and is not remotely valid.
#[derive(Serialize)]
pub struct Summary {
    pub fatals_before: usize,
    pub fatals_after: usize,
    pub errors_before: usize,
    pub errors_after: usize,
    pub applied: usize,
    pub skipped: usize,
    /// The bar this run was measured against: `valid` (the default — no error-
    /// and no fatal-severity findings remain) or `openable` (no fatals remain).
    /// Carried so `status: "ok"` is never read without it (CLI.md §6; a shared
    /// `goal` field waits for a second tool to need one).
    pub goal: &'static str,
}

impl Summary {
    fn of(report: &ChangeReport) -> Self {
        Summary {
            fatals_before: report.fatals_before,
            fatals_after: report.fatals_after,
            errors_before: report.errors_before,
            errors_after: report.errors_after,
            applied: report.applied().count(),
            skipped: report.skipped().count(),
            goal: report.goal.as_str(),
        }
    }
}

/// epubsana's `data` vocabulary. `tier` is its own axis — how much judgement a
/// fix needs, orthogonal to the severity it inherits — and `changes` are the
/// exact edits, the same list the human report prints.
#[derive(Serialize)]
pub struct Data {
    pub fix_id: &'static str,
    pub tier: &'static str,
    pub changes: Vec<String>,
}
