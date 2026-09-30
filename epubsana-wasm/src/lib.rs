//! WebAssembly bindings for [`epubsana`] — repair an EPUB's defects entirely in
//! the browser: no server round-trip, no upload. The bytes never leave the page.
//!
//! A stateful [`Session`] is where "confirm each step" lives for the web
//! frontend: load an EPUB, list the proposed fixes, let the user choose, then
//! run the repair and read back the repaired bytes. The async part is entirely
//! in the UI (waiting for clicks); every Rust call here is synchronous.
//!
//! **[`Session::repair`] is the core's `epubsana::repair`, not a copy of it.**
//! Until 0.20.0 the page applied each fix the moment it was approved and never
//! checked the result, so a fix that made the book worse went into the
//! download, and the same book with the same approvals could come back
//! different from the CLI. Now the page collects the approvals and hands them
//! to the same function the CLI runs, so a fix that raises a finding is undone
//! (`"reverted"`) here too, with the same exceptions for findings a fix
//! reveals (epubsana#7; asked for by epublift, 2026-09-30).
//!
//! [`Session::report`] returns the machine envelope's **`inputs[i]` shape**
//! (FORMATS.md §1.2) — minus the CLI-only `path`/`error` fields, since a JS
//! caller has neither. A caller therefore reads the *same* object the CLI's
//! `--format json` emits: one shape, one parser, across CLI, CI and the browser.
//! These structs mirror [`epubsana::envelope`]; keep them in step.
//!
//! ```js
//! import init, { Session } from "epubsana-wasm";
//! await init();
//! const s = Session.load(new Uint8Array(await file.arrayBuffer()));
//! const { fatals_before, errors_before, fixes } = s.plan();
//! const approved = fixes.filter((f) => f.tier === "AutoSafe").map((f) => f.index);
//! approved.push(2);                          // and a ConfirmNeeded one the user chose
//! const report = s.repair(approved, "valid"); // apply, check, undo what made it worse
//! s.revealed();                              // findings a fix let the validator see
//! const repaired = s.result_bytes();         // Uint8Array → download <name>_fixed.epub
//! ```

use serde::Serialize;
use tsify::Tsify;
use wasm_bindgen::prelude::*;

use std::collections::BTreeSet;

use epubsana::{
    ChangeReport, Confirmer, Decision, Goal, Outcome, Policy, ProposedFix, Tier, Workspace, fixers,
    workspace::Checkpoint,
};

/// One concrete edit a fix would make (mirrors `epubsana::Change`).
#[derive(Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Change {
    pub path: String,
    pub note: String,
}

/// A proposed fix as shown to the user. The apply logic stays in Rust; JS only
/// sees this description and passes `index` to `Session.repair`.
#[derive(Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Fix {
    /// This fix's handle in `Session.repair`'s `approved` list. **0-based** — it
    /// indexes this array.
    ///
    /// The envelope's [`Data::index`], on a report item, is the same fix's
    /// **1-based** plan position, because that is the handle `epubsana --apply`
    /// takes and the two documents have to agree. Do not pass one where the
    /// other is wanted.
    pub index: usize,
    /// `"AutoSafe"` (safe to auto-apply) or `"ConfirmNeeded"` (a visible change).
    /// epubsana's own axis: how much judgement the fix needs. Orthogonal to
    /// `severity`, which describes the *defect*.
    pub tier: String,
    /// The epubcheck-compatible ID this addresses, e.g. `"RSC-016"`.
    pub id: String,
    /// Lowercase severity of the finding this fix clears, **inherited** from
    /// epubveri: `"fatal" | "error" | "warning" | "info" | "usage"`. A fatal is
    /// what stops the book from opening at all.
    pub severity: String,
    /// One-line summary.
    pub title: String,
    /// Why the fix is safe / what the spec says.
    pub rationale: String,
    /// The exact edits it would make.
    pub preview: Vec<Change>,
    /// `"proposed"` until [`Session::repair`] runs; then `"applied"`,
    /// `"skipped"` (not approved) or `"reverted"` (approved, applied, and undone
    /// because it made a finding more frequent).
    pub outcome: String,
    /// For a `"reverted"` fix, the finding whose count applying it raised, as
    /// `"RSC-005 (opf.package.schema_violation)"` — the line the CLI prints.
    pub reverted_for: Option<String>,
}

/// The session's plan: the book's state before repair, and every proposed fix.
///
/// Fatals are counted apart from errors, as epubveri reports them — a book whose
/// defects are all fatal has `errors_before: 0` and does not even open.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Plan {
    pub fatals_before: usize,
    pub errors_before: usize,
    pub warnings_before: usize,
    pub fixes: Vec<Fix>,
}

/// The re-validated result — the envelope's `inputs[i]` object without
/// `path`/`error` (a wasm caller has no path, and in-memory bytes are always
/// readable, so there is no unprocessable/`"error"` case here).
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Report {
    /// `"ok"` (the goal was met) or `"problems"` (it was not).
    pub status: String,
    pub summary: Summary,
    pub items: Vec<Item>,
}

/// Per-input counts, mirroring the envelope's `summary`.
///
/// Including its naming rule, which is written out on `epubsana::envelope::Summary`:
/// a severity count here is **plural with a tense**, `<severity>s_before` /
/// `<severity>s_after`. Do not follow epubveri's singular `summary` keys when
/// adding one — theirs is a histogram of a single report, these are a repair
/// run's delta, and mixing the two spellings inside one object is the failure
/// that note exists to prevent. `Plan::warnings_before` is the shipped
/// precedent.
#[derive(Serialize, Tsify)]
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
    /// Planned and neither applied nor declined — every fix, before
    /// [`Session::repair`] has run. Counted because `outcome` is a closed set
    /// and a summary that reports some of its members disagrees with its own
    /// `items` (conventions #30, rule 1).
    pub proposed: usize,
    /// Approved and applied, then undone because applying it made a finding
    /// more frequent, exactly as in the CLI envelope. The identity is the CLI's:
    /// `applied + skipped + proposed + reverted == items.len()`.
    pub reverted: usize,
    /// The bar this result was measured against: `"valid"` or `"openable"`.
    pub goal: String,
}

/// One fix, in the shared item shape (FORMATS.md §1.3).
#[derive(Serialize, Tsify)]
pub struct Item {
    /// Always `"fix"` for a repairer.
    #[serde(rename = "type")]
    pub kind: String,
    /// `"applied"`, `"skipped"`, `"proposed"` or `"reverted"` — what became of
    /// this fix.
    /// Required on a fix item: a report that cannot say which fixes the user
    /// approved is not a report of what changed.
    pub outcome: String,
    /// epubcheck-compatible message ID this fix addresses, e.g. `"RSC-016"`.
    pub code: String,
    /// epubveri's finer semantic sub-code, when the finding carries one.
    pub rule: Option<String>,
    /// Lowercase severity, inherited from the finding this fix addresses.
    pub severity: String,
    /// Container-relative path the fix touches, when it touches just one.
    pub location: Option<String>,
    pub message: String,
    pub data: Data,
}

/// Tool-specific extras: epubsana's tier, and the exact edits.
#[derive(Serialize, Tsify)]
pub struct Data {
    /// This fix's **1-based** position in the plan, mirroring the CLI
    /// envelope's `data.index` — the selector `epubsana --apply` accepts.
    ///
    /// **Not the handle `Session.repair` takes.** That one is [`Fix::index`],
    /// which is 0-based because it indexes the `Plan.fixes` array JS already
    /// holds. The two differ on purpose: this field exists so a report saved
    /// from the browser is the *same document* a plugin gets from the CLI, and
    /// silently re-basing it would make the two disagree. Subtract one to cross
    /// between them.
    ///
    /// It was missing until 2026-08-22 — this struct is hand-written to get a
    /// TypeScript type out of Tsify rather than built on `epubsana::envelope`,
    /// so "keep them in step" (see the module docs) was a comment and not a
    /// mechanism. epubveri hit the identical gap in its own binding on the same
    /// day and told us to look.
    pub index: usize,
    pub fix_id: String,
    pub tier: String,
    pub changes: Vec<ChangeItem>,
}

/// One edit: the container entry it touches, and what it does there. Mirrors the
/// CLI's JSON exactly, so a browser consumer and a plugin read the same shape.
#[derive(Serialize, Tsify)]
pub struct ChangeItem {
    pub path: String,
    pub note: String,
}

/// A repair session over one EPUB, held in WASM memory across calls.
#[wasm_bindgen]
pub struct Session {
    ws: Workspace,
    /// The book as loaded. Every [`Session::repair`] starts from here, so a
    /// second call with different approvals replaces the first rather than
    /// stacking on top of it.
    start: Checkpoint,
    /// The last repair run, if any: outcomes and `revealed` come from it.
    last: Option<ChangeReport>,
    /// Stable, JS-facing descriptions, one per planned fix.
    infos: Vec<Fix>,
    /// What each planned fix addresses, for the envelope items.
    meta: Vec<Meta>,
    fatals_before: usize,
    errors_before: usize,
    warnings_before: usize,
    infos_before: usize,
    usages_before: usize,
}

/// The envelope fields of a planned fix that the JS-facing [`Fix`] does not
/// carry (kept out of the UI type, still needed for [`Session::report`]).
struct Meta {
    fix_id: &'static str,
    rule: Option<String>,
    location: Option<String>,
}

#[wasm_bindgen]
impl Session {
    /// Load an EPUB from its raw bytes, detect its defects, and plan the fixes.
    pub fn load(bytes: &[u8]) -> Result<Session, JsError> {
        let ws = Workspace::load(bytes).map_err(to_js)?;
        let report = ws.detect().map_err(to_js)?;
        let planned = fixers::plan(&report, &ws, Goal::Valid);
        let infos = planned
            .iter()
            .enumerate()
            .map(|(i, f)| describe(i, f))
            .collect();
        let meta = planned
            .iter()
            .map(|f| Meta {
                fix_id: f.fix_id,
                rule: f.addresses_rule.map(str::to_string),
                location: f.location(),
            })
            .collect();
        let start = ws.checkpoint();
        Ok(Session {
            ws,
            start,
            last: None,
            infos,
            meta,
            fatals_before: report.fatals(),
            errors_before: report.errors(),
            warnings_before: report.warnings(),
            infos_before: report.infos(),
            usages_before: report.usages(),
        })
    }

    /// The session's plan: the starting counts and every proposed fix (each with
    /// its current `outcome`). Cheap — it re-validates nothing.
    pub fn plan(&self) -> Plan {
        Plan {
            fatals_before: self.fatals_before,
            errors_before: self.errors_before,
            warnings_before: self.warnings_before,
            fixes: self.infos.clone(),
        }
    }

    /// Run the repair with the fixes at `approved` (0-based [`Fix::index`]
    /// values) and return the result — **the core's `epubsana::repair`, the
    /// function the CLI runs**, so the same book with the same approvals comes
    /// back identical from both.
    ///
    /// Every approved fix is applied, the book is validated, and a fix that
    /// made any finding more frequent is undone and reported `"reverted"`,
    /// except for findings a fix revealed (see [`Session::revealed`]). A fix not
    /// in `approved` is `"skipped"`, as it is under the CLI's `--apply`.
    ///
    /// Starts from the book as loaded every time, so calling it again with a
    /// different selection replaces the previous run. An index out of range is
    /// an error and changes nothing.
    pub fn repair(
        &mut self,
        approved: Vec<usize>,
        goal: Option<String>,
    ) -> Result<Report, JsError> {
        if approved.iter().any(|&i| i >= self.infos.len()) {
            return Err(JsError::new("fix index out of range"));
        }
        let goal = parse_goal(goal.as_deref());
        let run = self.run(approved.into_iter().collect(), goal)?;
        let report = self.envelope(
            goal,
            run.goal_met,
            [
                run.fatals_after,
                run.errors_after,
                run.warnings_after,
                run.infos_after,
                run.usages_after,
            ],
        );
        self.last = Some(run);
        Ok(report)
    }

    /// How many findings the last [`Session::repair`] accepted as **revealed**:
    /// always in the book, and visible only once a fix let the validator in —
    /// by making an unreadable document readable, or by correcting a package
    /// version no validator recognises. They were not in the plan, so running
    /// again on [`Session::result_bytes`] can repair some of them. `0` before
    /// any repair.
    ///
    /// Not in the envelope's `summary`, which the conventions own; the CLI
    /// prints the same number as a note after its report.
    pub fn revealed(&self) -> usize {
        self.last.as_ref().map_or(0, |r| r.revealed)
    }

    /// **Re-validate** the current EPUB with epubveri and return the result in
    /// the shared envelope shape — the same independent check the CLI reports.
    /// `goal` is `"valid"` (the default: no fatals and no errors remain) or
    /// `"openable"` (no fatals remain — the book opens). Before any
    /// [`Session::repair`] every item is `"proposed"` and the book is unchanged.
    pub fn report(&self, goal: Option<String>) -> Result<Report, JsError> {
        let goal = parse_goal(goal.as_deref());
        let after = self.ws.detect().map_err(to_js)?;
        Ok(self.envelope(
            goal,
            goal.is_met(&after),
            [
                after.fatals(),
                after.errors(),
                after.warnings(),
                after.infos(),
                after.usages(),
            ],
        ))
    }

    /// The repaired EPUB's bytes — download these as `<name>_fixed.epub`.
    pub fn result_bytes(&self) -> Result<Vec<u8>, JsError> {
        self.ws.serialize().map_err(to_js)
    }
}

impl Session {
    /// Rewind to the book as loaded and run the core repair with `approved`.
    fn run(&mut self, approved: BTreeSet<usize>, goal: Goal) -> Result<ChangeReport, JsError> {
        self.ws.seek(self.start);
        let mut confirmer = ByIndex { approved, next: 0 };
        let run = match epubsana::repair(&mut self.ws, goal, Policy::AskEach, &mut confirmer) {
            Ok(run) => run,
            Err(e) => {
                self.ws.seek(self.start);
                return Err(to_js(e));
            }
        };
        // The core planned again from the same bytes, and the indices the page
        // chose from are only meaningful if it planned the same thing. Planning
        // is deterministic, so this should never fire; if it does, nothing the
        // user did not see is left applied.
        let same = run.fixes.len() == self.infos.len()
            && run
                .fixes
                .iter()
                .zip(&self.infos)
                .all(|(r, f)| r.title == f.title && r.addresses_id == f.id);
        if !same {
            self.ws.seek(self.start);
            return Err(JsError::new(
                "the repair planned different fixes from the ones shown; nothing was applied",
            ));
        }
        for (f, r) in self.infos.iter_mut().zip(&run.fixes) {
            f.outcome = r.outcome.as_str().to_string();
            f.reverted_for = r.reverted_for.map(|(id, rule)| match rule {
                Some(rule) => format!("{id} ({rule})"),
                None => id.to_string(),
            });
        }
        Ok(run)
    }

    /// The envelope for the current outcomes, measured against `goal`.
    /// `after` is `[fatals, errors, warnings, infos, usages]`.
    fn envelope(&self, goal: Goal, goal_met: bool, after: [usize; 5]) -> Report {
        let items: Vec<Item> = self
            .infos
            .iter()
            .zip(&self.meta)
            .enumerate()
            .map(|(i, (f, m))| Item {
                kind: "fix".to_string(),
                outcome: f.outcome.clone(),
                code: f.id.clone(),
                rule: m.rule.clone(),
                severity: f.severity.clone(),
                location: m.location.clone(),
                message: f.title.clone(),
                data: Data {
                    index: i + 1,
                    fix_id: m.fix_id.to_string(),
                    tier: match f.tier.as_str() {
                        "AutoSafe" => "auto_safe",
                        _ => "confirm_needed",
                    }
                    .to_string(),
                    changes: f
                        .preview
                        .iter()
                        .map(|c| ChangeItem {
                            path: c.path.clone(),
                            note: c.note.clone(),
                        })
                        .collect(),
                },
            })
            .collect();
        let count = |o: Outcome| items.iter().filter(|i| i.outcome == o.as_str()).count();
        let [
            fatals_after,
            errors_after,
            warnings_after,
            infos_after,
            usages_after,
        ] = after;
        Report {
            status: if goal_met { "ok" } else { "problems" }.to_string(),
            summary: Summary {
                fatals_before: self.fatals_before,
                fatals_after,
                errors_before: self.errors_before,
                errors_after,
                warnings_before: self.warnings_before,
                warnings_after,
                infos_before: self.infos_before,
                infos_after,
                usages_before: self.usages_before,
                usages_after,
                applied: count(Outcome::Applied),
                skipped: count(Outcome::Skipped),
                proposed: count(Outcome::Proposed),
                reverted: count(Outcome::Reverted),
                goal: goal.as_str().to_string(),
            },
            items,
        }
    }
}

/// Approves the fixes whose plan position is in `approved`. The core asks once
/// per proposal, in plan order, so the position is the number of questions
/// asked so far.
struct ByIndex {
    approved: BTreeSet<usize>,
    next: usize,
}

impl Confirmer for ByIndex {
    fn decide(&mut self, _fix: &ProposedFix) -> Decision {
        let i = self.next;
        self.next += 1;
        if self.approved.contains(&i) {
            Decision::Approve
        } else {
            Decision::Reject
        }
    }
}

fn parse_goal(goal: Option<&str>) -> Goal {
    match goal {
        Some("openable") => Goal::Openable,
        _ => Goal::Valid,
    }
}

/// Build the JS-facing description of a proposed fix.
fn describe(index: usize, fix: &ProposedFix) -> Fix {
    Fix {
        index,
        tier: match fix.tier {
            Tier::AutoSafe => "AutoSafe",
            Tier::ConfirmNeeded => "ConfirmNeeded",
        }
        .to_string(),
        id: fix.addresses_id.clone(),
        severity: fix.addresses_severity.as_str().to_string(),
        title: fix.title.clone(),
        rationale: fix.rationale.clone(),
        preview: fix
            .preview
            .iter()
            .map(|c| Change {
                path: c.path.clone(),
                note: c.note.clone(),
            })
            .collect(),
        outcome: Outcome::Proposed.as_str().to_string(),
        reverted_for: None,
    }
}

fn to_js(e: epubsana::Error) -> JsError {
    JsError::new(&e.to_string())
}

/// The version string — the same one the CLI's `-V` and the json envelope's
/// `tool_version` print, git build metadata included.
#[wasm_bindgen]
pub fn version() -> String {
    epubsana::VERSION.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use epubsana::{Confirmer, Decision, ProposedFix};
    use std::io::Write;

    fn epub(files: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let stored = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file("mimetype", stored).unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            for (name, body) in files {
                zip.start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    const CONTAINER: &str = r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;

    const NCX: &str = r#"<?xml version="1.0" encoding="utf-8"?><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head><meta name="dtb:uid" content="urn:uuid:1"/></head><docTitle><text>t</text></docTitle><navMap><navPoint id="1n" playOrder="1"><navLabel><text>One</text></navLabel><content src="c1.xhtml"/></navPoint></navMap></ncx>"#;

    /// An EPUB 2 book with `version` substituted and `body` as its one
    /// chapter's body.
    fn book(version: &str, body: &str) -> Vec<u8> {
        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="{version}" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>t</dc:title><dc:identifier id="id">urn:uuid:1</dc:identifier><dc:language>en</dc:language></metadata><manifest><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine toc="ncx"><itemref idref="c1"/></spine></package>"#
        );
        let doc = format!(
            r#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>c</title></head><body>{body}</body></html>"#
        );
        epub(&[
            ("META-INF/container.xml", CONTAINER),
            ("content.opf", &opf),
            ("toc.ncx", NCX),
            ("c1.xhtml", &doc),
        ])
    }

    struct ApproveAll;
    impl Confirmer for ApproveAll {
        fn decide(&mut self, _: &ProposedFix) -> Decision {
            Decision::Approve
        }
    }

    /// Two independent fixes in two files: an undeclared entity in the chapter
    /// and an NCX id that is not a valid NCName, so approvals can be told apart.
    fn two_fix_book() -> Vec<u8> {
        book("2.0", "<p>a&nbsp;b</p><p><a name=\"x\" id=\"x\">y</a></p>")
    }

    /// **The property this binding exists to keep**: the same book with the
    /// same approvals comes back byte for byte what the CLI's library call
    /// produces, with the same outcomes.
    #[test]
    fn a_browser_run_and_a_cli_run_produce_the_same_book() {
        let bytes = two_fix_book();

        let mut ws = Workspace::load(&bytes).unwrap();
        let cli = epubsana::repair(&mut ws, Goal::Valid, Policy::AskEach, &mut ApproveAll).unwrap();
        assert!(cli.fixes.len() >= 2, "fixture must plan at least two fixes");

        let mut s = Session::load(&bytes).unwrap();
        let all: Vec<usize> = (0..s.plan().fixes.len()).collect();
        let r = s.repair(all, None).unwrap();

        assert_eq!(s.result_bytes().unwrap(), ws.serialize().unwrap());
        let outcomes: Vec<_> = r.items.iter().map(|i| i.outcome.as_str()).collect();
        let cli_outcomes: Vec<_> = cli.fixes.iter().map(|f| f.outcome.as_str()).collect();
        assert_eq!(outcomes, cli_outcomes);
        assert_eq!(r.summary.errors_after, cli.errors_after);
        assert_eq!(r.summary.fatals_after, cli.fatals_after);
    }

    /// A fix the page did not approve is `"skipped"`, as under `--apply`, and
    /// the four outcome counts add up to the item count.
    #[test]
    fn an_unapproved_fix_is_skipped() {
        let mut s = Session::load(&two_fix_book()).unwrap();
        let n = s.plan().fixes.len();
        let r = s.repair(vec![0], None).unwrap();
        assert_eq!(r.items[0].outcome, "applied");
        assert!(r.items[1..].iter().all(|i| i.outcome == "skipped"));
        let m = &r.summary;
        assert_eq!(m.applied + m.skipped + m.proposed + m.reverted, n);
        assert_eq!(s.plan().fixes[1].outcome, "skipped");
    }

    /// A second run replaces the first rather than stacking on it: running
    /// {0} and then {1} gives the book a single run of {1} gives.
    #[test]
    fn a_second_repair_starts_again_from_the_book_as_loaded() {
        let bytes = two_fix_book();
        let mut twice = Session::load(&bytes).unwrap();
        twice.repair(vec![0], None).unwrap();
        twice.repair(vec![1], None).unwrap();

        let mut once = Session::load(&bytes).unwrap();
        once.repair(vec![1], None).unwrap();

        assert_eq!(twice.result_bytes().unwrap(), once.result_bytes().unwrap());
        assert_eq!(twice.plan().fixes[0].outcome, "skipped");
    }

    /// Before any repair nothing is applied and every item is `"proposed"`.
    #[test]
    fn before_a_repair_everything_is_proposed() {
        let bytes = two_fix_book();
        let s = Session::load(&bytes).unwrap();
        let r = s.report(None).unwrap();
        assert!(r.items.iter().all(|i| i.outcome == "proposed"));
        assert_eq!(r.summary.errors_after, r.summary.errors_before);
        assert_eq!(s.revealed(), 0);
        assert_eq!(
            s.result_bytes().unwrap(),
            Workspace::load(&bytes).unwrap().serialize().unwrap()
        );
    }

    /// The version fix lets validation start, and what it finds is reported
    /// as revealed — the fix is applied, not reverted, exactly as in the CLI.
    #[test]
    fn a_revealing_fix_is_kept_and_counted() {
        // `<blink/>` is invisible while the unknown version stops validation.
        let bytes = book("1.0", "<p>a</p><blink/>");
        let mut s = Session::load(&bytes).unwrap();
        let plan = s.plan();
        let v = plan
            .fixes
            .iter()
            .find(|f| f.id == "OPF-001")
            .expect("the version fix must be planned");
        let r = s.repair(vec![v.index], None).unwrap();
        assert_eq!(r.items[v.index].outcome, "applied");
        assert!(
            s.revealed() > 0,
            "nothing was revealed, so this proves nothing"
        );
    }
}
