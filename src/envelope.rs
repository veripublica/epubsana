//! epubsana's `--format json` — the veripublica machine envelope, built on
//! **epubveri's reference types** ([`epubveri::envelope`], FORMATS.md
//! convention v0.6).
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

/// The FORMATS.md convention version **this crate implements** — not the one
/// its detector implements.
///
/// epubveri 0.13.x hard-coded its own `CONVENTION` into every envelope built
/// through [`epubveri::envelope::Envelope::for_tool`], so epubsana's output was
/// claiming epubveri's convention version; it was right by accident, both being
/// `"0.4"`. 0.14.0 made the key a required parameter precisely so that raising
/// the dependency cannot be mistaken for adopting a convention release, and
/// epubveri now claims `"0.5"`.
///
/// **`"0.6"` since 0.17.0, which emits `reverted`** (conventions v0.6.0, #31) —
/// the release that added per-fix rollback. It was `"0.5"` from the release that
/// shipped [`Summary`]'s seven missing counters. The key moves when this crate implements a convention release, never
/// when the dependency does: it is an assertion about ourselves (FORMATS.md
/// §1.1, settled by conventions on 2026-09-10).
const CONVENTION: &str = "0.6";

/// epubsana's `Outcome` in the envelope's vocabulary.
///
/// The conversion — rather than a shared enum — is what epubsana asked
/// conventions for on 2026-09-10 and what epubveri 0.14.0 shipped: a tool keeps
/// its own vocabulary, and the *place the two meet* becomes something the
/// compiler forces you to revisit. **The `match` must stay wildcard-free**: a
/// `_ =>` arm turns that compile error back into the silence the type exists to
/// prevent, exactly as `violation_kind` does upstream.
impl From<crate::Outcome> for epubveri::envelope::Outcome {
    fn from(o: crate::Outcome) -> Self {
        match o {
            crate::Outcome::Applied => epubveri::envelope::Outcome::Applied,
            crate::Outcome::Skipped => epubveri::envelope::Outcome::Skipped,
            crate::Outcome::Proposed => epubveri::envelope::Outcome::Proposed,
            crate::Outcome::Reverted => epubveri::envelope::Outcome::Reverted,
        }
    }
}

/// epubsana's envelope, with its two tool-owned slots filled in.
pub type Envelope = epubveri::envelope::Envelope<Summary, Data>;
/// One repaired input, in the shared shape.
pub type Input = epubveri::envelope::Input<Summary, Data>;

/// Build the whole run's envelope: one input (a transformer takes exactly one),
/// `dry_run` set when nothing was written on purpose.
pub fn envelope(input: Input, dry_run: bool) -> Envelope {
    let mut env = Envelope::for_tool("epubsana", crate::VERSION, CONVENTION, None, vec![input]);
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
        items: report
            .fixes
            .iter()
            .enumerate()
            .map(|(i, f)| item(i + 1, f))
            .collect(),
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
fn item(index: usize, f: &ReportedFix) -> Item<Data> {
    Item::fix(
        f.outcome,
        f.addresses_id.clone(),
        f.addresses_rule,
        f.addresses_severity.as_str(),
        f.location.clone(),
        None, // a fix spans a file, not a point in it
        f.title.clone(),
        Some(Data {
            index,
            fix_id: f.fix_id,
            tier: match f.tier {
                Tier::AutoSafe => "auto_safe",
                Tier::ConfirmNeeded => "confirm_needed",
            },
            changes: f
                .changes
                .iter()
                .map(|c| ChangeItem {
                    path: c.path.clone(),
                    note: c.note.clone(),
                })
                .collect(),
        }),
    )
}

/// epubsana's `summary` vocabulary (tool-owned; a consumer MUST NOT require it).
///
/// Fatals are counted apart from errors, as epubveri reports them: a book whose
/// defects are all fatal has `errors_before: 0` and is not remotely valid.
///
/// # Naming: a severity count here is `<severity>s_before` / `<severity>s_after`
///
/// **Plural, and settled by precedent rather than by preference** — the wasm
/// binding's `Plan` has shipped `warnings_before` since it existed. Anything
/// added later follows the same shape.
///
/// The rule is written down because of one specific way it would otherwise be
/// broken. epubveri's `summary` keys went **singular** in its 0.11.0 —
/// `fatal`, `error`, `warning`, `info`, `usage` — on the good argument that
/// *information* has no plural and *usages* is not English. Someone adding a
/// severity to this struct will meet that convention first and follow it, and
/// the result is `warning_before` sitting next to `errors_before` **inside one
/// object**. An inconsistency within a single summary is worse than one between
/// two tools, and it is the one thing this note exists to prevent.
///
/// The two are not the same slot, which is why matching them would be the wrong
/// fix rather than merely a costly one. epubveri's keys are a **histogram of one
/// report** — how many findings of each severity a book has. These are the
/// **before/after delta of a repair run**, which is why each name carries a
/// tense. FORMATS.md makes `summary` tool-owned precisely so the two can differ.
///
/// What is genuinely missing is a dimension, not a spelling: the human report
/// counts warnings and this does not, so a machine consumer cannot see the work
/// of the three fixers that run on `usage`/`warning` findings — 41 proposals and
/// 147 findings cleared across 385 books, against a Δ-errors of exactly zero.
/// Adding them is additive and cheap now that `ChangeReport::before` is kept.
/// Deliberately not done here; the naming is decided, the addition is not.
#[derive(Serialize)]
pub struct Summary {
    pub fatals_before: usize,
    pub fatals_after: usize,
    pub errors_before: usize,
    pub errors_after: usize,
    pub warnings_before: usize,
    pub warnings_after: usize,
    pub infos_before: usize,
    pub infos_after: usize,
    pub usages_before: usize,
    pub usages_after: usize,
    pub applied: usize,
    pub skipped: usize,
    /// Planned and neither applied nor declined. **Every member of `outcome`'s
    /// closed set is counted, including zero** — a `--dry-run` used to report
    /// `applied: 0, skipped: 0` while every item carried `"outcome":
    /// "proposed"`, so the summary and the items disagreed about the size of
    /// the run and every number in the document was true.
    pub proposed: usize,
    /// Approved and applied, then undone because applying it produced a new
    /// finding (conventions #31, epubsana#7). The identity a consumer may rely
    /// on is therefore `applied + skipped + proposed + reverted == items.len()`.
    ///
    /// It was deliberately absent, not zero, while no build could revert: `0`
    /// would have claimed no revert happened where the truth was that none
    /// could. It is reported now because the concept exists.
    pub reverted: usize,
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
            warnings_before: report.warnings_before,
            warnings_after: report.warnings_after,
            infos_before: report.infos_before,
            infos_after: report.infos_after,
            usages_before: report.usages_before,
            usages_after: report.usages_after,
            applied: report.applied().count(),
            skipped: report.skipped().count(),
            proposed: report.proposed().count(),
            reverted: report.reverted().count(),
            goal: report.goal.as_str(),
        }
    }
}

/// epubsana's `data` vocabulary. `tier` is its own axis — how much judgement a
/// fix needs, orthogonal to the severity it inherits — and `changes` are the
/// exact edits, the same list the human report prints. `index` is the handle
/// `--apply` accepts, so a consumer can act on a subset of what it was shown.
#[derive(Serialize)]
pub struct Data {
    /// This fix's 1-based position in the plan — the selector `--apply` takes.
    ///
    /// Planning is deterministic (same input, same detector version, same plan,
    /// same order), which is what makes an index a usable handle across the two
    /// invocations a plugin needs: one `--dry-run` to show the proposals, one
    /// `--apply` to act on the subset the user picked.
    pub index: usize,
    pub fix_id: &'static str,
    pub tier: &'static str,
    pub changes: Vec<ChangeItem>,
}

/// One edit a fix makes: **which container entry** it touches, and a human
/// description of what it does there.
///
/// The path is the half that took longest to arrive. Until 0.10.0 this was a
/// bare string — the description only — which meant a plugin wanting to know
/// *which files changed* had to open the repaired EPUB and compare every entry
/// against the original. Doitsu said so on MobileRead, and he was right: the
/// path was already in `Change` and the emitter was throwing it away.
///
/// A fix that spans files produces one entry per file, so `changes` is also the
/// answer to "what would this fix touch" without applying anything.
///
/// **`path` is not always a file whose *content* changed.** The packaging fix
/// (`PKG-006`) reports `mimetype` because that is the entry it acts on, while
/// the entry's bytes stay identical — only its position in the archive and its
/// compression change. A consumer that writes changed files back cannot
/// reproduce that fix by copying `mimetype`; it has to re-save the container, or
/// use epubsana's own output.
#[derive(Serialize)]
pub struct ChangeItem {
    /// Container-relative path of the entry this edit touches.
    pub path: String,
    /// Human description of the edit, e.g. "replace `&mdash;` → `—` (88×)".
    pub note: String,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{Confirmer, Decision, Goal, Policy, ProposedFix, Workspace, repair};
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    /// A small EPUB 3 carrying one `error` and two `usage` findings, one of
    /// each reachable by a fixer.
    ///
    /// The two severities are the point. `PKG-006` moves the error line;
    /// `OPF-090` (a non-preferred font media type) is repaired without moving
    /// it at all, which is exactly the work a summary reporting only fatals and
    /// errors describes as nothing. A fixture with errors alone would let every
    /// assertion below pass while the new counters stayed dead.
    pub(crate) fn fixture_epub() -> Vec<u8> {
        const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bookid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="bookid">urn:uuid:11111111-2222-3333-4444-555555555555</dc:identifier>
    <dc:title>Fixture</dc:title>
    <dc:language>en</dc:language>
    <meta property="dcterms:modified">2026-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>
    <item id="f1" href="f.ttf" media-type="application/x-font-ttf"/>
  </manifest>
  <spine><itemref idref="c1"/></spine>
</package>
"#;
        const NAV: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>nav</title></head>
<body><nav epub:type="toc"><ol><li><a href="c1.xhtml">One</a></li></ol></nav></body></html>
"#;
        const DOC: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>One</title></head><body><p>Hello</p></body></html>
"#;
        const CONTAINER: &str = r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));
            let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            let deflated =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zip.start_file("META-INF/container.xml", stored).unwrap();
            zip.write_all(CONTAINER.as_bytes()).unwrap();
            // `mimetype` neither first nor stored: PKG-006, the error half.
            zip.start_file("mimetype", deflated).unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            zip.start_file("content.opf", stored).unwrap();
            zip.write_all(OPF.as_bytes()).unwrap();
            zip.start_file("nav.xhtml", stored).unwrap();
            zip.write_all(NAV.as_bytes()).unwrap();
            zip.start_file("c1.xhtml", stored).unwrap();
            zip.write_all(DOC.as_bytes()).unwrap();
            // A `glyf` sfnt, so the declared type is non-preferred rather than wrong.
            zip.start_file("f.ttf", stored).unwrap();
            zip.write_all(&[0x00, 0x01, 0x00, 0x00]).unwrap();
            zip.write_all(&[0u8; 60]).unwrap();
            zip.finish().unwrap();
        }
        buf
    }

    struct ApproveAll;
    impl Confirmer for ApproveAll {
        fn decide(&mut self, _: &ProposedFix) -> Decision {
            Decision::Approve
        }
    }

    struct RejectAll;
    impl Confirmer for RejectAll {
        fn decide(&mut self, _: &ProposedFix) -> Decision {
            Decision::Reject
        }
    }

    fn summary_of(policy: Policy, confirmer: &mut dyn Confirmer) -> (Summary, usize) {
        let mut ws = Workspace::load(&fixture_epub()).unwrap();
        let report = repair(&mut ws, Goal::Valid, policy, confirmer).unwrap();
        let n = report.fixes.len();
        (Summary::of(&report), n)
    }

    /// **The identity a consumer is invited to rely on**, across all three run
    /// shapes. It is the whole point of adding `proposed`: before it, a
    /// `--dry-run` reported `applied: 0, skipped: 0` beside a non-empty `items`,
    /// so this sum was short by exactly the proposals.
    #[test]
    fn applied_skipped_and_proposed_account_for_every_item() {
        for (policy, confirmer) in [
            (Policy::DryRun, &mut ApproveAll as &mut dyn Confirmer),
            (Policy::AskEach, &mut ApproveAll),
            (Policy::AskEach, &mut RejectAll),
        ] {
            let (s, items) = summary_of(policy, confirmer);
            assert!(items > 0, "the fixture must plan something to count");
            assert_eq!(
                s.applied + s.skipped + s.proposed + s.reverted,
                items,
                "summary and items disagree about the size of the run"
            );
        }
    }

    /// A dry run proposes and neither applies nor declines — the exact shape
    /// that used to report two zeroes and say nothing about the rest.
    #[test]
    fn a_dry_run_counts_its_proposals() {
        let (s, items) = summary_of(Policy::DryRun, &mut ApproveAll);
        assert_eq!(s.proposed, items);
        assert_eq!((s.applied, s.skipped), (0, 0));
    }

    /// Every severity is reported in both tenses, not just the two the verdict
    /// is computed from. The fixture's own `usage` findings are the witness:
    /// the check would pass vacuously against a book that had none.
    #[test]
    fn all_five_severities_are_reported_in_both_tenses() {
        let mut ws = Workspace::load(&fixture_epub()).unwrap();
        let report = repair(&mut ws, Goal::Valid, Policy::DryRun, &mut RejectAll).unwrap();
        let s = Summary::of(&report);

        assert_eq!(s.fatals_before, report.before.fatals());
        assert_eq!(s.errors_before, report.before.errors());
        assert_eq!(s.warnings_before, report.before.warnings());
        assert_eq!(s.infos_before, report.before.infos());
        assert_eq!(s.usages_before, report.before.usages());

        assert!(
            s.usages_before > 0,
            "fixture carries no finding below `error`, so this test proves nothing"
        );
        // A declined dry run writes nothing, so every `after` equals its `before`.
        assert_eq!(
            (s.warnings_after, s.infos_after, s.usages_after),
            (s.warnings_before, s.infos_before, s.usages_before)
        );
    }

    /// A repair the error line reports as nothing still shows in the summary.
    ///
    /// This is the measured reason rule 1 was read the wider way here rather
    /// than stopping at `proposed`: `fix.non_preferred_media_type` clears a
    /// `usage` finding and moves neither `errors_*` nor `fatals_*`, and three
    /// fixers in this crate are of that kind. Before these fields the document
    /// described that work as no change at all.
    #[test]
    fn a_usage_only_repair_is_visible_in_the_summary() {
        let mut ws = Workspace::load(&fixture_epub()).unwrap();
        let report = repair(&mut ws, Goal::Valid, Policy::AskEach, &mut ApproveAll).unwrap();
        let s = Summary::of(&report);

        assert!(
            report
                .applied()
                .any(|f| f.fix_id == "fix.non_preferred_media_type"),
            "the usage-severity fixer did not run, so this test proves nothing"
        );
        assert!(
            s.usages_after < s.usages_before,
            "a cleared usage finding left the summary unchanged"
        );
    }

    /// The stability key is an assertion about **this** crate, so it moves when
    /// epubsana implements a convention release and never when the dependency
    /// does. Pinned so a dependency bump cannot quietly carry it.
    #[test]
    fn the_convention_key_is_our_own() {
        assert_eq!(CONVENTION, "0.6");
    }

    /// Every outcome the envelope can carry is one epubsana can produce.
    ///
    /// Our `From` is wildcard-free, so a member added on *our* side fails to
    /// compile until it is mapped. This covers the other direction: a member
    /// added to the convention reaches us through `epubveri::envelope::Outcome`
    /// and compiles silently. `ALL` is the tripwire epubveri ships for exactly
    /// this (0.17.0); a fifth value needs a decision here, not a new list entry.
    #[test]
    fn we_can_emit_every_outcome_the_envelope_defines() {
        use epubveri::envelope::Outcome as Theirs;
        let ours = [
            crate::Outcome::Applied,
            crate::Outcome::Skipped,
            crate::Outcome::Proposed,
            crate::Outcome::Reverted,
        ];
        for t in Theirs::ALL {
            assert!(
                ours.iter().any(|o| Theirs::from(*o) == *t),
                "the envelope defines {t:?} and epubsana never emits it"
            );
        }
    }

    /// A reverted fix reaches the json as `"reverted"`, not as `"skipped"`,
    /// and the summary still accounts for every item. Driven through a real
    /// revert — a fix injected to damage the book — because the shelf never
    /// produces one, so nothing else would exercise this path.
    #[test]
    fn a_reverted_fix_is_reported_as_reverted_in_the_envelope() {
        let mut ws = Workspace::load(&fixture_epub()).unwrap();
        let report = crate::repair_with(
            &mut ws,
            Goal::Valid,
            Policy::AskEach,
            &mut ApproveAll,
            &crate::tests::good_bad_good,
        )
        .unwrap();
        let env = envelope(input("in.epub".into(), None, &report), false);
        let json: serde_json::Value = serde_json::to_value(&env).unwrap();
        let input = &json["inputs"][0];
        let outcomes: Vec<&str> = input["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["outcome"].as_str().unwrap())
            .collect();
        assert_eq!(outcomes, ["applied", "reverted", "applied"]);
        let s = &input["summary"];
        assert_eq!(s["reverted"], 1);
        assert_eq!(
            ["applied", "skipped", "proposed", "reverted"]
                .iter()
                .map(|k| s[k].as_u64().unwrap())
                .sum::<u64>(),
            outcomes.len() as u64
        );
    }
}
