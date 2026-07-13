//! epubsana — repairs the EPUB defects [epubveri](https://crates.io/crates/epubveri)
//! detects.
//!
//! A pure-Rust companion to epubveri. epubveri *finds* what's wrong
//! (by epubcheck-compatible message ID, with a stable `rule` sub-code and exact
//! position); epubsana turns the safely-fixable findings into **proposed edits
//! the caller approves one by one**, applies the approved ones, and emits a
//! **report of exactly what changed**.
//!
//! The heart is a UI-agnostic contract so every frontend (CLI, a WASM page,
//! epublift) behaves identically:
//! - [`Workspace`] — the fidelity-preserving in-memory EPUB.
//! - [`ProposedFix`] — what a fix would do; it does not mutate until approved.
//! - [`Confirmer`] — the frontend decides, per fix (this is how "confirm each
//!   step" lives in the core).
//! - [`ChangeReport`] — what actually changed, fix by fix, with each one's
//!   [`Outcome`].
//!
//! Invariants: nothing mutates without an approved [`ProposedFix`]; edits are
//! surgical and content-preserving; and a fix is only ever proposed when a safe
//! one exists — we never guess.

pub mod entities;
pub mod envelope;
pub mod fixers;
pub mod workspace;

pub use workspace::{Error, Workspace};

use epubveri::report::{Report, Severity};

/// The crate version, carrying git build metadata (`+<short-hash>[.dirty]`) when
/// built from a checkout — the one string the CLI's `-V`, the json envelope's
/// `tool_version` and the wasm binding's `version()` all print (veripublica
/// conventions v0.4, CLI.md §3.1). A build with no git (e.g. a crates.io
/// tarball) falls back silently to the plain SemVer, set by `build.rs`.
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), env!("EPUBSANA_BUILD"));

/// How much intervention a fix needs — mirrors the feasibility-spike tiers.
///
/// Orthogonal to the *severity* of the finding it addresses: a trivially safe
/// fix can clear a fatal, and a fix needing a decision can clear a warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Exactly one correct, content-preserving fix; safe to auto-apply.
    AutoSafe,
    /// A good fix exists but involves a choice/default — the caller should
    /// approve it explicitly.
    ConfirmNeeded,
}

/// What happened to a proposed fix — the shared item field the machine envelope
/// requires on every `fix` (FORMATS.md §1.3, conventions v0.4, issue #25).
///
/// A confirm-each-step repairer mixes these within one ordinary run, which is
/// exactly why the fact is per-item and not a property of the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The change was made.
    Applied,
    /// Presented and not done: the caller declined.
    Skipped,
    /// No decision exists yet — a dry run.
    Proposed,
}

impl Outcome {
    /// The lowercase spelling the shared json envelope uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Applied => "applied",
            Outcome::Skipped => "skipped",
            Outcome::Proposed => "proposed",
        }
    }
}

/// One concrete edit a fix makes (or would make), for previews and the report.
#[derive(Debug, Clone)]
pub struct Change {
    /// Container entry the edit touches.
    pub path: String,
    /// Human description of the edit (e.g. "replace `&mdash;` → `—` (88×)").
    pub note: String,
}

/// What a fix would do, built from one epubveri finding. Carries a preview but
/// does **not** mutate the [`Workspace`] until [`ProposedFix::apply`] is called
/// (only after the caller approves it).
pub struct ProposedFix {
    /// Stable fixer identifier, e.g. `"fix.html_entities"`.
    pub fix_id: &'static str,
    /// The epubveri message ID this addresses (e.g. `"RSC-016"`).
    pub addresses_id: String,
    /// The epubveri `rule` sub-code this addresses, if any. `&'static str`: a
    /// fixer dispatches on a compile-time rule, and the shared envelope's `rule`
    /// field is `&'static str` too, so it passes straight through.
    pub addresses_rule: Option<&'static str>,
    /// The severity epubveri gave that finding. A fix **inherits** it verbatim —
    /// it is never a judgement about the fix itself (FORMATS.md §1.3).
    pub addresses_severity: Severity,
    /// How much intervention this fix needs.
    pub tier: Tier,
    /// One-line human summary.
    pub title: String,
    /// Why this fix is safe / what the spec says.
    pub rationale: String,
    /// The edits this fix would make.
    pub preview: Vec<Change>,
    apply_fn: Box<dyn FnOnce(&mut Workspace)>,
}

impl ProposedFix {
    /// Apply the fix to the workspace (call only after approval).
    pub fn apply(self, ws: &mut Workspace) {
        (self.apply_fn)(ws)
    }

    /// The container entry this fix touches, when all its edits touch one file —
    /// the item's `location` in the machine envelope.
    pub fn location(&self) -> Option<String> {
        let first = &self.preview.first()?.path;
        self.preview
            .iter()
            .all(|c| &c.path == first)
            .then(|| first.clone())
    }
}

/// The caller's decision on a single [`ProposedFix`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Approve,
    Reject,
}

/// The frontend implements this — it IS how "confirm each step" lives in the
/// core. Given a fix and its preview, return a [`Decision`].
pub trait Confirmer {
    fn decide(&mut self, fix: &ProposedFix) -> Decision;
}

/// How far to repair — and, with it, what counts as success.
///
/// `Valid` is the default and means what a verifier means by it: no error- and
/// no fatal-severity findings remain. `Openable` is the **explicitly-requested
/// lesser goal** the convention allows (CLI.md §6): the e-reader / fix-on-import
/// bar, *"at least make it open"*. Under it, exit `0` can coexist with
/// error-severity findings in the report — the exit code answers the question
/// the invocation asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Goal {
    Openable,
    #[default]
    Valid,
}

impl Goal {
    /// Whether a detection report meets this goal.
    ///
    /// `Openable` keys on **fatals alone**, and that is not a proxy: a fatal is
    /// precisely the class of defect that stops an EPUB from being processed at
    /// all — an unreadable ZIP, a missing `container.xml` or OPF, XHTML that is
    /// not well-formed, an unterminated entity reference. Everything below it a
    /// reading system renders anyway (it runs no RelaxNG). Zero fatals *is* the
    /// book opening.
    pub fn is_met(self, report: &Report) -> bool {
        match self {
            Goal::Valid => report.is_valid(),
            Goal::Openable => report.fatals() == 0,
        }
    }

    /// The `--goal` spelling, for help text and the machine envelope.
    pub fn as_str(self) -> &'static str {
        match self {
            Goal::Valid => "valid",
            Goal::Openable => "openable",
        }
    }
}

/// Batch policy layered over the [`Confirmer`], so a caller need not answer
/// every trivial `AutoSafe` fix by hand while still getting a full report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// Ask the confirmer for every fix.
    AskEach,
    /// Auto-apply `AutoSafe` fixes; ask the confirmer for `ConfirmNeeded` ones.
    AutoSafeThenAsk,
    /// Propose and report, but apply nothing.
    DryRun,
}

/// One fix as it appears in the end-of-run report: what it would do, and what
/// became of it.
#[derive(Debug, Clone)]
pub struct ReportedFix {
    pub fix_id: &'static str,
    pub addresses_id: String,
    pub addresses_rule: Option<&'static str>,
    pub addresses_severity: Severity,
    pub tier: Tier,
    pub title: String,
    /// Why the fix is safe / what the spec says — the same text the confirm
    /// prompt shows, kept so a `--dry-run` report can justify each proposal.
    pub rationale: String,
    pub location: Option<String>,
    pub changes: Vec<Change>,
    /// Applied, skipped (declined), or merely proposed (a dry run).
    pub outcome: Outcome,
}

/// The end-of-run record — the second half of the "confirm + report" contract.
///
/// Fatals are counted apart from errors, exactly as epubveri reports them: a
/// book whose only defects are fatal has `errors_before == 0` and is not
/// remotely valid. Reading only the error counts is the trap the five-value
/// severity vocabulary introduced, and epubsana's flagship fixer (undeclared
/// HTML entities) clears *fatals*.
#[derive(Debug, Clone, Default)]
pub struct ChangeReport {
    /// Every planned fix, in proposal order, each carrying its [`Outcome`].
    pub fixes: Vec<ReportedFix>,
    pub fatals_before: usize,
    pub fatals_after: usize,
    pub errors_before: usize,
    pub errors_after: usize,
    /// The bar this run was measured against.
    pub goal: Goal,
    /// Whether the run's [`Goal`] was met by the re-validated result — the
    /// tool's `0`/`1` line.
    pub goal_met: bool,
}

impl ChangeReport {
    pub fn applied(&self) -> impl Iterator<Item = &ReportedFix> {
        self.with_outcome(Outcome::Applied)
    }

    pub fn skipped(&self) -> impl Iterator<Item = &ReportedFix> {
        self.with_outcome(Outcome::Skipped)
    }

    fn with_outcome(&self, outcome: Outcome) -> impl Iterator<Item = &ReportedFix> {
        self.fixes.iter().filter(move |f| f.outcome == outcome)
    }

    /// Whether anything was actually written to the workspace.
    pub fn changed(&self) -> bool {
        self.applied().next().is_some()
    }
}

/// Detect with epubveri, propose fixes for the findings, ask the caller per
/// fix (subject to `policy`), apply the approved ones, and return a report.
///
/// v1 note: fixes are applied then the whole book is re-validated for the
/// before/after counts. Per-fix transactional rollback (apply → re-validate →
/// undo if it introduced any new error) is the next hardening step.
pub fn repair(
    ws: &mut Workspace,
    goal: Goal,
    policy: Policy,
    confirmer: &mut dyn Confirmer,
) -> Result<ChangeReport, Error> {
    let before = ws.detect()?;
    let (fatals_before, errors_before) = (before.fatals(), before.errors());

    let proposals = fixers::plan(&before, ws, goal);
    let mut fixes = Vec::new();

    for fix in proposals {
        let outcome = match policy {
            Policy::DryRun => Outcome::Proposed,
            Policy::AutoSafeThenAsk if fix.tier == Tier::AutoSafe => Outcome::Applied,
            Policy::AutoSafeThenAsk | Policy::AskEach => match confirmer.decide(&fix) {
                Decision::Approve => Outcome::Applied,
                Decision::Reject => Outcome::Skipped,
            },
        };
        fixes.push(ReportedFix {
            fix_id: fix.fix_id,
            addresses_id: fix.addresses_id.clone(),
            addresses_rule: fix.addresses_rule,
            addresses_severity: fix.addresses_severity,
            tier: fix.tier,
            title: fix.title.clone(),
            rationale: fix.rationale.clone(),
            location: fix.location(),
            changes: fix.preview.clone(),
            outcome,
        });
        if outcome == Outcome::Applied {
            fix.apply(ws);
        }
    }

    // Re-validate: the before/after counts and the verdict are epubveri's, never
    // epubsana's own bookkeeping. A fix that did not clear its finding says so.
    let after = ws.detect()?;
    Ok(ChangeReport {
        fixes,
        fatals_before,
        fatals_after: after.fatals(),
        errors_before,
        errors_after: after.errors(),
        goal,
        goal_met: goal.is_met(&after),
    })
}
