//! WebAssembly bindings for [`epubsana`] — repair an EPUB's defects entirely in
//! the browser: no server round-trip, no upload. The bytes never leave the page.
//!
//! A stateful [`Session`] is where "confirm each step" lives for the web
//! frontend: load an EPUB, list the proposed fixes, apply them one at a time (or
//! auto-apply the provably-safe ones), then read back the repaired bytes. The
//! async part is entirely in the UI (waiting for clicks); every Rust call here
//! is synchronous, and the repair logic is exactly the core's — never duplicated.
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
//! s.apply_auto_safe();               // apply the AutoSafe ones in one go
//! s.apply(2);                        // approve a specific ConfirmNeeded fix
//! const report = s.report("valid");  // re-validates: status, summary, items[]
//! const repaired = s.result_bytes(); // Uint8Array → download <name>_fixed.epub
//! ```

use serde::Serialize;
use tsify_next::Tsify;
use wasm_bindgen::prelude::*;

use epubsana::{fixers, Goal, Outcome, ProposedFix, Tier, Workspace};

/// One concrete edit a fix would make (mirrors `epubsana::Change`).
#[derive(Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Change {
    pub path: String,
    pub note: String,
}

/// A proposed fix as shown to the user. The apply logic stays in Rust; JS only
/// sees this description and calls `Session.apply(index)`.
#[derive(Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Fix {
    /// Index to pass to `Session.apply`.
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
    /// `"applied"` once approved in this session, else `"proposed"`.
    pub outcome: String,
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
#[derive(Serialize, Tsify)]
pub struct Summary {
    pub fatals_before: usize,
    pub fatals_after: usize,
    pub errors_before: usize,
    pub errors_after: usize,
    pub applied: usize,
    pub skipped: usize,
    /// The bar this result was measured against: `"valid"` or `"openable"`.
    pub goal: String,
}

/// One fix, in the shared item shape (FORMATS.md §1.3).
#[derive(Serialize, Tsify)]
pub struct Item {
    /// Always `"fix"` for a repairer.
    #[serde(rename = "type")]
    pub kind: String,
    /// `"applied"`, `"skipped"`, or `"proposed"` — what became of this fix.
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
    pub fix_id: String,
    pub tier: String,
    pub changes: Vec<String>,
}

/// A repair session over one EPUB, held in WASM memory across calls.
#[wasm_bindgen]
pub struct Session {
    ws: Workspace,
    /// Each planned fix, taken (`None`) once applied.
    fixes: Vec<Option<ProposedFix>>,
    /// Stable, JS-facing descriptions, kept even after a fix is applied.
    infos: Vec<Fix>,
    /// What each planned fix addresses, for the envelope items.
    meta: Vec<Meta>,
    fatals_before: usize,
    errors_before: usize,
    warnings_before: usize,
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
        Ok(Session {
            ws,
            fixes: planned.into_iter().map(Some).collect(),
            infos,
            meta,
            fatals_before: report.fatals(),
            errors_before: report.errors(),
            warnings_before: report.warnings(),
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

    /// Apply the fix at `index` (a user-approved step). A no-op if that fix was
    /// already applied; an error only if `index` is out of range.
    pub fn apply(&mut self, index: usize) -> Result<(), JsError> {
        let slot = self
            .fixes
            .get_mut(index)
            .ok_or_else(|| JsError::new("fix index out of range"))?;
        if let Some(fix) = slot.take() {
            fix.apply(&mut self.ws);
            self.infos[index].outcome = Outcome::Applied.as_str().to_string();
        }
        Ok(())
    }

    /// Apply every provably-safe (`AutoSafe`) fix at once; returns how many were
    /// applied. The shortcut for "just fix what's unambiguously safe."
    pub fn apply_auto_safe(&mut self) -> usize {
        let mut applied = 0;
        for i in 0..self.fixes.len() {
            if self.infos[i].tier == "AutoSafe" {
                if let Some(fix) = self.fixes[i].take() {
                    fix.apply(&mut self.ws);
                    self.infos[i].outcome = Outcome::Applied.as_str().to_string();
                    applied += 1;
                }
            }
        }
        applied
    }

    /// **Re-validate** the current (possibly repaired) EPUB with epubveri and
    /// return the result in the shared envelope shape — the same independent
    /// check the CLI reports. `goal` is `"valid"` (the default: no fatals and no
    /// errors remain) or `"openable"` (no fatals remain — the book opens).
    pub fn report(&self, goal: Option<String>) -> Result<Report, JsError> {
        let goal = match goal.as_deref() {
            Some("openable") => Goal::Openable,
            _ => Goal::Valid,
        };
        let after = self.ws.detect().map_err(to_js)?;

        let items: Vec<Item> = self
            .infos
            .iter()
            .zip(&self.meta)
            .map(|(f, m)| Item {
                kind: "fix".to_string(),
                outcome: f.outcome.clone(),
                code: f.id.clone(),
                rule: m.rule.clone(),
                severity: f.severity.clone(),
                location: m.location.clone(),
                message: f.title.clone(),
                data: Data {
                    fix_id: m.fix_id.to_string(),
                    tier: match f.tier.as_str() {
                        "AutoSafe" => "auto_safe",
                        _ => "confirm_needed",
                    }
                    .to_string(),
                    changes: f.preview.iter().map(|c| c.note.clone()).collect(),
                },
            })
            .collect();

        let applied = items.iter().filter(|i| i.outcome == "applied").count();
        Ok(Report {
            status: if goal.is_met(&after) {
                "ok"
            } else {
                "problems"
            }
            .to_string(),
            summary: Summary {
                fatals_before: self.fatals_before,
                fatals_after: after.fatals(),
                errors_before: self.errors_before,
                errors_after: after.errors(),
                applied,
                skipped: 0, // the browser never declines: an unapplied fix stays proposed
                goal: goal.as_str().to_string(),
            },
            items,
        })
    }

    /// The repaired EPUB's bytes — download these as `<name>_fixed.epub`.
    pub fn result_bytes(&self) -> Result<Vec<u8>, JsError> {
        self.ws.serialize().map_err(to_js)
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
