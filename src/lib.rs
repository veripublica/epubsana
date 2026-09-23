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

// No unsafe code exists in this crate, and a repairer that parses hostile
// input has no reason to grow any: this makes that a compile error rather
// than a convention.
#![forbid(unsafe_code)]

pub mod entities;
pub mod envelope;
pub mod fixers;
pub mod workspace;

pub use workspace::{Error, Workspace};

use epubveri::report::{Report, Severity};

/// The crate version, carrying git build metadata (`+<short-hash>[.dirty]`) when
/// built from a checkout — the one string the CLI's `-V`, the json envelope's
/// `tool_version` and the wasm binding's `version()` all print (veripublica
/// conventions v0.6, CLI.md §3.1). A build with no git (e.g. a crates.io
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
/// requires on every `fix` (FORMATS.md §1.3, conventions v0.6, issues #25 and #31).
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
    /// Approved and applied, then undone by the tool because applying it
    /// produced a finding that was not there before (conventions #31). The fix
    /// is **not** in the output and the caller did **not** decline it; the
    /// finding it addressed is unrepaired. See [`repair`].
    Reverted,
}

impl Outcome {
    /// The lowercase spelling the shared json envelope uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Applied => "applied",
            Outcome::Skipped => "skipped",
            Outcome::Proposed => "proposed",
            Outcome::Reverted => "reverted",
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
    /// Applied, skipped (declined), merely proposed (a dry run), or reverted.
    pub outcome: Outcome,
    /// For a [`Outcome::Reverted`] fix, the `(id, rule)` of the finding whose
    /// count applying it raised — the reason it was undone. `None` otherwise.
    pub reverted_for: Option<(&'static str, Option<&'static str>)>,
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
    /// The detection report the run started from — **every** finding the book
    /// had, not only the ones a fixer could reach.
    ///
    /// It used to be computed, reduced to `fatals_before`/`errors_before`, and
    /// dropped. Keeping it is the first step toward the routing display, and
    /// epubveri's framing of that work is the right one: the missing thing was
    /// never "a findings view", it was that the data had already been thrown
    /// away by the time anything could ask for it.
    ///
    /// **What it enables.** `No fixes to propose.` currently collapses two
    /// opposite claims into one sentence — *a human repairs this in an editor*
    /// (a choice about the book: which of two languages, which identifier is
    /// canonical, where a broken link meant to point) and *nobody should repair
    /// this automatically*. The second is the stronger thing this tool can say
    /// and today it cannot say it. Grouping these findings by
    /// `(violation_kind, params[0])` is how the display will say which.
    ///
    /// **What it does not enable yet, so nobody builds on the assumption.**
    /// There is no link from a [`ReportedFix`] back to the findings it cleared.
    /// A fixer groups many findings into one proposal — 28 navigation points in
    /// one NCX became one fix — and only the `id`/`rule` it dispatched on
    /// survives. So "the findings we did *not* address" cannot be computed
    /// exactly from this field; matching on `(id, rule)` is an approximation and
    /// must not be presented as the residue. Recording the consumed findings per
    /// fixer is the next step, and it is the same missing primitive per-fix
    /// rollback needs (issue #7).
    pub before: epubveri::report::Report,
    pub fatals_before: usize,
    pub fatals_after: usize,
    pub errors_before: usize,
    pub errors_after: usize,
    /// The three severities below `error`, in both tenses.
    ///
    /// They are here because the counts a repairer reports are a **closed set
    /// the tool has a concept of**, and reporting two of its five members makes
    /// the document say less than it knows (conventions #30, rule 1, read the
    /// wider way on this project's own catch). It is not a hypothetical gap:
    /// three fixers here dispatch on `usage`/`warning` findings, so a run can
    /// clear real work and leave `errors_before`/`errors_after` unmoved — the
    /// value metric is *user burden reduced*, and the error line is a verdict
    /// metric, not a value one.
    ///
    /// Cheap by construction: [`repair`] already re-validates to compute the
    /// error and fatal deltas, and used to drop every other count on the floor.
    pub warnings_before: usize,
    pub warnings_after: usize,
    pub infos_before: usize,
    pub infos_after: usize,
    pub usages_before: usize,
    pub usages_after: usize,
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

    /// Planned, shown, and neither applied nor declined — every fix of a
    /// `--dry-run`, and the sibling `applied`/`skipped` were missing. Without
    /// it a summary answers "how many fixes were there" with zero while the
    /// items list two.
    pub fn proposed(&self) -> impl Iterator<Item = &ReportedFix> {
        self.with_outcome(Outcome::Proposed)
    }

    /// Approved and applied, then undone because applying it produced a new
    /// finding. See [`repair`].
    pub fn reverted(&self) -> impl Iterator<Item = &ReportedFix> {
        self.with_outcome(Outcome::Reverted)
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
/// # A fix that makes the book worse is undone (issue #7)
///
/// After the approved fixes are applied the book is validated once. If any
/// `(id, rule)` now occurs **more often than before** — at any severity, since
/// the value metric is the user's burden and not only the verdict — the run
/// bisects the [`Workspace`] history to the first fix whose application raised
/// it, drops that fix, and replays the rest. It repeats until nothing has
/// risen. A dropped fix is reported [`Outcome::Reverted`], never
/// [`Outcome::Skipped`]: the caller approved it and the tool overruled.
///
/// - **"Rose", not "is new".** A fix that trades one finding for another of a
///   kind the book already carries — the `fix.content_properties` regression
///   traded an OPF-014 for an RSC-005 — creates no new key, only a larger count.
/// - **A fix that clears a fatal is not undone for what it reveals.** Clearing
///   a fatal lets the detector into a document for the first time, and what it
///   finds was always in the book (Baris, 2026-09-22) — a whitespace-only
///   `<title>` behind an undeclared entity, or a broken `#fragment` in *another*
///   document that points into it, which the detector could not check while the
///   target was unreadable. So when the bisection pins a rise on a fix under
///   which the fatal count fell, the rise is accepted as revealed and the
///   comparison carries on from there; a later fix raising the same key further
///   is still caught. The line is drawn at the fix, not at the finding, because
///   a finding-level match would have to know every site at which the detector
///   skips an unreadable target — seven today, two of which name the target only
///   as the raw href (epubveri, 2026-09-22). The price: a defect a
///   fatal-clearing fix genuinely authors, anywhere in the book, is accepted
///   too. Today only the two entity fixers clear fatals, and both only replace
///   entity references with characters.
/// - **Replaying re-plans.** [`ProposedFix::apply`] consumes the fix, so after a
///   revert the workspace returns to where the run started and the same
///   detection is planned again. Planning is deterministic (see
///   [`fixers::plan`]), and the replay checks it: if the second plan differs in
///   any proposal, the run stops with nothing applied rather than apply a plan
///   the caller never saw.
/// - Bisection is only paid for when something rose: a run in which nothing
///   does costs the same two validations it always did.
///
/// What this cannot catch is a defect the detector does not recognise — the
/// re-validation uses the same detector that approved the fix.
pub fn repair(
    ws: &mut Workspace,
    goal: Goal,
    policy: Policy,
    confirmer: &mut dyn Confirmer,
) -> Result<ChangeReport, Error> {
    repair_with(ws, goal, policy, confirmer, &fixers::plan)
}

/// A planner: what [`fixers::plan`] is, and what a test substitutes to inject
/// a fix that damages the book.
type Planner<'a> = &'a dyn Fn(&Report, &Workspace, Goal) -> Vec<ProposedFix>;

fn repair_with(
    ws: &mut Workspace,
    goal: Goal,
    policy: Policy,
    confirmer: &mut dyn Confirmer,
    planner: Planner,
) -> Result<ChangeReport, Error> {
    let before = ws.detect()?;
    let (fatals_before, errors_before) = (before.fatals(), before.errors());
    let (warnings_before, infos_before, usages_before) =
        (before.warnings(), before.infos(), before.usages());

    let start = ws.checkpoint();
    let proposals = planner(&before, ws, goal);
    let mut fixes = Vec::new();
    for fix in &proposals {
        let outcome = match policy {
            Policy::DryRun => Outcome::Proposed,
            Policy::AutoSafeThenAsk if fix.tier == Tier::AutoSafe => Outcome::Applied,
            Policy::AutoSafeThenAsk | Policy::AskEach => match confirmer.decide(fix) {
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
            reverted_for: None,
        });
    }

    let mut proposals = Some(proposals);
    let after = 'replay: loop {
        let plan = match proposals.take() {
            Some(p) => p,
            None => {
                let p = planner(&before, ws, goal);
                if !same_plan(&p, &fixes) {
                    return Err(Error::Nondeterministic(
                        "re-planning after a revert produced a different plan; nothing was applied"
                            .into(),
                    ));
                }
                p
            }
        };
        // `marks[k]` is the history position after the first k applied fixes.
        let mut marks = vec![ws.checkpoint()];
        let mut applied = Vec::new();
        for (i, fix) in plan.into_iter().enumerate() {
            if fixes[i].outcome == Outcome::Applied {
                fix.apply(ws);
                marks.push(ws.checkpoint());
                applied.push(i);
            }
        }
        let after = ws.detect()?;
        // Every state reached is detected at most once per replay. Prefix 0 is
        // the starting bytes, so its findings are `before`'s; with nothing
        // applied a risen key is therefore impossible.
        let mut seen = std::collections::BTreeMap::new();
        seen.insert(0, Tally::of(&before));
        seen.insert(applied.len(), Tally::of(&after));
        // Rebuilt on every replay: an accepted rise was measured against a set
        // of applied fixes that a revert has since changed.
        let mut baseline = Tally::of(&before);
        let last = applied.len();
        while let Some(key) = baseline.first_risen(&seen[&last]) {
            // Invariant: `lo` fixes do not raise `key` past the baseline, `hi` do.
            let (mut lo, mut hi) = (0, last);
            while hi - lo > 1 {
                let mid = (lo + hi) / 2;
                if baseline.rose(tally_at(ws, &marks, &mut seen, mid)?, &key) {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            let fatals_under = tally_at(ws, &marks, &mut seen, hi - 1)?.fatals;
            let at_hi = tally_at(ws, &marks, &mut seen, hi)?;
            if at_hi.fatals < fatals_under {
                // Revealed, not authored: accept this much of the rise.
                let n = at_hi.count(&key);
                baseline.counts.insert(key, n);
                continue;
            }
            let culprit = &mut fixes[applied[hi - 1]];
            culprit.outcome = Outcome::Reverted;
            culprit.reverted_for = Some(key);
            ws.seek(start);
            continue 'replay;
        }
        ws.seek(marks[last]);
        break after;
    };

    Ok(ChangeReport {
        fixes,
        fatals_before,
        fatals_after: after.fatals(),
        errors_before,
        errors_after: after.errors(),
        warnings_before,
        warnings_after: after.warnings(),
        infos_before,
        infos_after: after.infos(),
        usages_before,
        usages_after: after.usages(),
        goal,
        goal_met: goal.is_met(&after),
        before,
    })
}

/// Whether a re-plan proposes exactly what the caller was shown and decided on.
fn same_plan(plan: &[ProposedFix], shown: &[ReportedFix]) -> bool {
    plan.len() == shown.len()
        && plan.iter().zip(shown).all(|(p, s)| {
            p.fix_id == s.fix_id
                && p.addresses_id == s.addresses_id
                && p.addresses_rule == s.addresses_rule
                && p.preview.len() == s.changes.len()
                && p.preview
                    .iter()
                    .zip(&s.changes)
                    .all(|(a, b)| a.path == b.path && a.note == b.note)
        })
}

type Key = (&'static str, Option<&'static str>);

/// Findings counted by `(id, rule)`, plus the fatal count the revealed-rise
/// rule compares. See [`repair`].
struct Tally {
    counts: std::collections::BTreeMap<Key, usize>,
    fatals: usize,
}

impl Tally {
    fn of(r: &Report) -> Tally {
        let mut counts = std::collections::BTreeMap::new();
        for m in &r.messages {
            *counts.entry((m.id, m.rule)).or_insert(0) += 1;
        }
        Tally {
            counts,
            fatals: r.fatals(),
        }
    }

    fn count(&self, key: &Key) -> usize {
        self.counts.get(key).copied().unwrap_or(0)
    }

    fn rose(&self, now: &Tally, key: &Key) -> bool {
        now.count(key) > self.count(key)
    }

    /// The first key, in sorted order, that occurs more often in `now`.
    fn first_risen(&self, now: &Tally) -> Option<Key> {
        now.counts.keys().find(|k| self.rose(now, k)).copied()
    }
}

/// The tally of the state after the first `k` applied fixes, detecting it only
/// if this replay has not already.
fn tally_at<'a>(
    ws: &mut Workspace,
    marks: &[workspace::Checkpoint],
    seen: &'a mut std::collections::BTreeMap<usize, Tally>,
    k: usize,
) -> Result<&'a Tally, Error> {
    if let std::collections::btree_map::Entry::Vacant(slot) = seen.entry(k) {
        ws.seek(marks[k]);
        slot.insert(Tally::of(&ws.detect()?));
    }
    Ok(&seen[&k])
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    /// A container broken in several unrelated ways at once, so the run has both
    /// findings a fixer reaches and findings it does not.
    fn broken_epub() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));
            let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            let deflated =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            // mimetype neither first nor stored: PKG-006.
            zip.start_file("mimetype", deflated).unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            zip.start_file("META-INF/container.xml", stored).unwrap();
            zip.write_all(b"<container/>").unwrap();
            zip.finish().unwrap();
        }
        buf
    }

    struct RejectAll;
    impl Confirmer for RejectAll {
        fn decide(&mut self, _: &ProposedFix) -> Decision {
            Decision::Reject
        }
    }

    /// The detection report the run started from is kept, not reduced to two
    /// integers and dropped.
    ///
    /// The second and third assertions are the load-bearing ones: they pin the
    /// retained report and the counts the CLI prints to the *same* detection, so
    /// the two can never drift into disagreeing about the same book. Everything
    /// the routing display will say is derived from this field, and a summary
    /// that contradicts the findings under it would be worse than no display.
    #[test]
    fn the_before_report_survives_the_run() {
        let mut ws = Workspace::load(&broken_epub()).unwrap();
        let report = repair(&mut ws, Goal::Valid, Policy::DryRun, &mut RejectAll).unwrap();

        assert!(
            !report.before.messages.is_empty(),
            "a deliberately broken container must produce findings"
        );
        assert_eq!(report.before.fatals(), report.fatals_before);
        assert_eq!(report.before.errors(), report.errors_before);
    }

    // ---- issue #7: a fix that makes the book worse is undone -------------

    use crate::envelope::tests::fixture_epub;
    use std::cell::Cell;

    struct ApproveAll;
    impl Confirmer for ApproveAll {
        fn decide(&mut self, _: &ProposedFix) -> Decision {
            Decision::Approve
        }
    }

    fn fix(
        id: &'static str,
        path: &str,
        apply: impl FnOnce(&mut Workspace) + 'static,
    ) -> ProposedFix {
        ProposedFix {
            fix_id: id,
            addresses_id: "TEST".into(),
            addresses_rule: None,
            addresses_severity: Severity::Error,
            tier: Tier::ConfirmNeeded,
            title: id.into(),
            rationale: String::new(),
            preview: vec![Change {
                path: path.into(),
                note: id.into(),
            }],
            apply_fn: Box::new(apply),
        }
    }

    fn rewrite(
        path: &'static str,
        from: &'static str,
        to: &'static str,
    ) -> impl FnOnce(&mut Workspace) {
        move |ws: &mut Workspace| {
            let t = ws.get_text(path).unwrap();
            assert!(t.contains(from), "fixture no longer contains {from:?}");
            ws.set_text(path, t.replacen(from, to, 1));
        }
    }

    /// Two good fixes around one that authors a schema violation. The bad one
    /// sits in the middle so the bisection has to find it rather than fall on
    /// an end, and the one after it proves a revert does not take its
    /// neighbours with it — the replay re-applies them.
    pub(crate) fn good_bad_good(_: &Report, _: &Workspace, _: Goal) -> Vec<ProposedFix> {
        vec![
            fix("good.mimetype", "mimetype", |ws| ws.repackage_mimetype()),
            fix(
                "bad.blink",
                "c1.xhtml",
                rewrite("c1.xhtml", "<p>Hello</p>", "<p>Hello</p><blink/>"),
            ),
            fix(
                "good.font",
                "content.opf",
                rewrite("content.opf", "application/x-font-ttf", "font/ttf"),
            ),
        ]
    }

    #[test]
    fn a_fix_that_adds_a_finding_is_reverted_and_its_neighbours_are_kept() {
        let mut ws = Workspace::load(&fixture_epub()).unwrap();
        let r = repair_with(
            &mut ws,
            Goal::Valid,
            Policy::AskEach,
            &mut ApproveAll,
            &good_bad_good,
        )
        .unwrap();

        let outcomes: Vec<_> = r.fixes.iter().map(|f| (f.fix_id, f.outcome)).collect();
        assert_eq!(
            outcomes,
            [
                ("good.mimetype", Outcome::Applied),
                ("bad.blink", Outcome::Reverted),
                ("good.font", Outcome::Applied),
            ]
        );
        assert!(r.fixes[1].reverted_for.is_some());
        assert!(!ws.get_text("c1.xhtml").unwrap().contains("blink"));
        assert!(ws.get_text("content.opf").unwrap().contains("font/ttf"));
        // Nothing the run kept added a finding: the output is no worse anywhere.
        assert!(r.errors_after < r.errors_before);
        assert_eq!(
            r.applied().count() + r.skipped().count() + r.proposed().count() + r.reverted().count(),
            r.fixes.len()
        );
    }

    /// The trigger is a count that rose, not a key that is new: the book already
    /// carries one `<blink/>`, so the bad fix adds no new `(id, rule)` — only a
    /// second instance of one. This is the shape of the `fix.content_properties`
    /// regression, which traded an OPF-014 for an RSC-005.
    #[test]
    fn a_fix_that_grows_an_existing_finding_is_reverted() {
        let mut ws = Workspace::load(&fixture_epub()).unwrap();
        let t = ws.get_text("c1.xhtml").unwrap();
        ws.set_text(
            "c1.xhtml",
            t.replace("<p>Hello</p>", "<blink/><p>Hello</p>"),
        );
        let before = ws.serialize().unwrap();

        let r = repair_with(
            &mut ws,
            Goal::Valid,
            Policy::AskEach,
            &mut ApproveAll,
            &good_bad_good,
        )
        .unwrap();
        assert_eq!(r.fixes[1].outcome, Outcome::Reverted);
        assert_eq!(
            ws.get_text("c1.xhtml").unwrap().matches("<blink/>").count(),
            1,
            "the reverted fix's second <blink/> reached the output"
        );
        assert_ne!(
            ws.serialize().unwrap(),
            before,
            "the good fixes were lost too"
        );
    }

    /// A fix that clears a fatal lets the parser into a document for the first
    /// time, and what it finds there was always in the book. Here the entity
    /// is undeclared (fatal), and behind it sits a `<blink/>` the detector
    /// cannot see until the entity is gone. Reverting would leave the book
    /// unopenable to avoid reporting a defect it already had.
    #[test]
    fn a_finding_revealed_by_clearing_a_fatal_does_not_revert_the_fix() {
        let mut ws = Workspace::load(&fixture_epub()).unwrap();
        let t = ws.get_text("c1.xhtml").unwrap();
        ws.set_text(
            "c1.xhtml",
            t.replace("<p>Hello</p>", "<p>Hello&bogus;</p><blink/>"),
        );
        let r0 = ws.detect().unwrap();
        assert!(
            r0.fatals() > 0,
            "fixture must start with a fatal, or this proves nothing"
        );

        // Between two unrelated fixes, so accepting the rise bisects into the
        // middle of the history and the run must come back out to its end.
        let plan = |_: &Report, _: &Workspace, _: Goal| {
            vec![
                fix("good.mimetype", "mimetype", |ws| ws.repackage_mimetype()),
                fix(
                    "clear.entity",
                    "c1.xhtml",
                    rewrite("c1.xhtml", "&bogus;", ""),
                ),
                fix(
                    "good.font",
                    "content.opf",
                    rewrite("content.opf", "application/x-font-ttf", "font/ttf"),
                ),
            ]
        };
        let r = repair_with(
            &mut ws,
            Goal::Valid,
            Policy::AskEach,
            &mut ApproveAll,
            &plan,
        )
        .unwrap();
        assert!(r.fixes.iter().all(|f| f.outcome == Outcome::Applied));
        assert_eq!(r.fatals_after, 0);
        assert!(
            ws.detect().unwrap().messages.iter().any(|m| m.location.as_deref()
                == Some("c1.xhtml")
                && m.severity == Severity::Error),
            "the <blink/> was never revealed, so this proves nothing"
        );
        assert!(
            ws.get_text("content.opf").unwrap().contains("font/ttf"),
            "the workspace was left at an intermediate state"
        );
    }

    /// The shape epubveri found (2026-09-22): the revealed finding is not in
    /// the document the fatal was in. While `c1.xhtml` cannot be parsed the
    /// detector has no ids for it and does not check fragments pointing into
    /// it, so the nav's broken `#missing` link appears only once the entity is
    /// gone — located in `nav.xhtml`, where no match on the fatal's location
    /// could have found it.
    #[test]
    fn a_finding_revealed_in_another_document_does_not_revert_the_fix() {
        let mut ws = Workspace::load(&fixture_epub()).unwrap();
        let t = ws.get_text("c1.xhtml").unwrap();
        ws.set_text("c1.xhtml", t.replace("<p>Hello</p>", "<p>Hello&bogus;</p>"));
        let nav = ws.get_text("nav.xhtml").unwrap();
        ws.set_text(
            "nav.xhtml",
            nav.replace("href=\"c1.xhtml\"", "href=\"c1.xhtml#missing\""),
        );

        let plan = |_: &Report, _: &Workspace, _: Goal| {
            vec![fix(
                "clear.entity",
                "c1.xhtml",
                rewrite("c1.xhtml", "&bogus;", ""),
            )]
        };
        let r = repair_with(
            &mut ws,
            Goal::Valid,
            Policy::AskEach,
            &mut ApproveAll,
            &plan,
        )
        .unwrap();
        let after = ws.detect().unwrap();
        assert!(
            after
                .messages
                .iter()
                .any(|m| m.location.as_deref() == Some("nav.xhtml")
                    && m.rule == Some("opf.content_document.dangling_fragment")),
            "the cross-document finding was never revealed, so this proves nothing"
        );
        assert_eq!(r.fixes[0].outcome, Outcome::Applied);
        assert_eq!(r.fatals_after, 0);
    }

    /// Accepting what a fatal-clearing fix revealed must not blind the run to a
    /// later fix that raises the same key further. Both touch one `<blink/>`
    /// count: the first reveals one, the second authors another.
    #[test]
    fn a_rise_beyond_what_was_revealed_is_still_reverted() {
        let mut ws = Workspace::load(&fixture_epub()).unwrap();
        let t = ws.get_text("c1.xhtml").unwrap();
        ws.set_text(
            "c1.xhtml",
            t.replace("<p>Hello</p>", "<p>Hello&bogus;</p><blink/>"),
        );

        let plan = |_: &Report, _: &Workspace, _: Goal| {
            vec![
                fix(
                    "clear.entity",
                    "c1.xhtml",
                    rewrite("c1.xhtml", "&bogus;", ""),
                ),
                fix(
                    "bad.blink",
                    "c1.xhtml",
                    rewrite("c1.xhtml", "<blink/>", "<blink/><blink/>"),
                ),
            ]
        };
        let r = repair_with(
            &mut ws,
            Goal::Valid,
            Policy::AskEach,
            &mut ApproveAll,
            &plan,
        )
        .unwrap();
        let outcomes: Vec<_> = r.fixes.iter().map(|f| f.outcome).collect();
        assert_eq!(outcomes, [Outcome::Applied, Outcome::Reverted]);
        let c1 = ws.get_text("c1.xhtml").unwrap();
        assert_eq!(c1.matches("<blink/>").count(), 1);
        assert!(
            !c1.contains("&bogus;"),
            "the workspace was left at an intermediate state"
        );
    }

    /// If replaying after a revert does not reproduce the plan the caller
    /// decided on, nothing is applied: applying a plan nobody approved is worse
    /// than applying none.
    #[test]
    fn a_replay_that_plans_differently_applies_nothing() {
        let pristine = fixture_epub();
        let mut ws = Workspace::load(&pristine).unwrap();
        let calls = Cell::new(0);
        let plan = |r: &Report, w: &Workspace, g: Goal| {
            calls.set(calls.get() + 1);
            let mut p = good_bad_good(r, w, g);
            if calls.get() > 1 {
                p.pop();
            }
            p
        };
        let r = repair_with(
            &mut ws,
            Goal::Valid,
            Policy::AskEach,
            &mut ApproveAll,
            &plan,
        );
        assert!(matches!(r, Err(Error::Nondeterministic(_))));
        assert_eq!(
            ws.serialize().unwrap(),
            Workspace::load(&pristine).unwrap().serialize().unwrap()
        );
    }

    /// Nothing rose, so nothing is bisected or replayed: the planner runs once.
    #[test]
    fn a_clean_run_plans_once() {
        let mut ws = Workspace::load(&fixture_epub()).unwrap();
        let calls = Cell::new(0);
        let plan = |r: &Report, w: &Workspace, g: Goal| {
            calls.set(calls.get() + 1);
            let mut p = good_bad_good(r, w, g);
            p.remove(1);
            p
        };
        let r = repair_with(
            &mut ws,
            Goal::Valid,
            Policy::AskEach,
            &mut ApproveAll,
            &plan,
        )
        .unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(r.reverted().count(), 0);
    }
}
