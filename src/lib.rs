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
//! - [`ChangeReport`] — what actually changed.
//!
//! Invariants: nothing mutates without an approved [`ProposedFix`]; edits are
//! surgical and content-preserving; and a fix is only ever proposed when a safe
//! one exists — we never guess.

pub mod entities;
pub mod fixers;
pub mod workspace;

pub use workspace::{Error, Workspace};

/// How much intervention a fix needs — mirrors the feasibility-spike tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Exactly one correct, content-preserving fix; safe to auto-apply.
    AutoSafe,
    /// A good fix exists but involves a choice/default — the caller should
    /// approve it explicitly.
    ConfirmNeeded,
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
    /// The epubveri `rule` sub-code this addresses, if any.
    pub addresses_rule: Option<String>,
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

/// How far to repair. `Openable` is the e-reader "at least it opens" bar;
/// `Valid` targets full epubcheck validity. (v1 proposes the same fixers for
/// both; the distinction will gate fixer selection as the registry grows.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Goal {
    Openable,
    Valid,
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

/// A fix that was applied, with the edits it made.
#[derive(Debug, Clone)]
pub struct AppliedFix {
    pub fix_id: &'static str,
    pub title: String,
    pub changes: Vec<Change>,
}

/// The end-of-run record — the second half of the "confirm + report" contract.
#[derive(Debug, Clone, Default)]
pub struct ChangeReport {
    pub applied: Vec<AppliedFix>,
    /// Titles of fixes the caller declined (or that a `DryRun` left).
    pub skipped: Vec<String>,
    pub errors_before: usize,
    pub errors_after: usize,
}

/// Detect with epubveri, propose fixes for the findings, ask the caller per
/// fix (subject to `policy`), apply the approved ones, and return a report.
///
/// v1 note: fixes are applied then the whole book is re-validated for the
/// before/after counts. Per-fix transactional rollback (apply → re-validate →
/// undo if it introduced any new error) is the next hardening step; the only
/// v1 fixer (HTML-entity mapping) is provably non-regressing.
pub fn repair(
    ws: &mut Workspace,
    goal: Goal,
    policy: Policy,
    confirmer: &mut dyn Confirmer,
) -> Result<ChangeReport, Error> {
    let before = ws.detect()?;
    let errors_before = before.errors();

    let proposals = fixers::plan(&before, ws, goal);
    let mut applied = Vec::new();
    let mut skipped = Vec::new();

    for fix in proposals {
        let approve = match policy {
            Policy::DryRun => false,
            Policy::AutoSafeThenAsk if fix.tier == Tier::AutoSafe => true,
            Policy::AutoSafeThenAsk | Policy::AskEach => {
                confirmer.decide(&fix) == Decision::Approve
            }
        };
        if approve {
            applied.push(AppliedFix {
                fix_id: fix.fix_id,
                title: fix.title.clone(),
                changes: fix.preview.clone(),
            });
            fix.apply(ws);
        } else {
            skipped.push(fix.title.clone());
        }
    }

    let errors_after = ws.detect()?.errors();
    Ok(ChangeReport {
        applied,
        skipped,
        errors_before,
        errors_after,
    })
}
