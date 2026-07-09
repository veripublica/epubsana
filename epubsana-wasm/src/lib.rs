//! WebAssembly bindings for [`epubsana`] — repair an EPUB's defects entirely in
//! the browser: no JVM, no server round-trip, no upload. The bytes never leave
//! the page.
//!
//! A stateful [`Session`] is where "confirm each step" lives for the web
//! frontend: load an EPUB, list the proposed fixes, apply them one at a time (or
//! auto-apply the provably-safe ones), then read back the repaired bytes. The
//! async part is entirely in the UI (waiting for clicks); every Rust call here
//! is synchronous, and the repair logic is exactly the core's — never duplicated.
//!
//! ```js
//! import init, { Session } from "epubsana-wasm";
//! await init();
//! const s = Session.load(new Uint8Array(await file.arrayBuffer()));
//! const { errors_before, fixes } = s.state();
//! s.apply_auto_safe();               // apply the AutoSafe ones in one go
//! s.apply(2);                        // approve a specific ConfirmNeeded fix
//! const remaining = s.errors_after();
//! const repaired = s.result_bytes(); // Uint8Array → download <name>_fixed.epub
//! ```

use serde::Serialize;
use tsify_next::Tsify;
use wasm_bindgen::prelude::*;

use epubsana::{fixers, Goal, ProposedFix, Tier, Workspace};

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
    pub tier: String,
    /// The epubcheck-compatible ID this addresses, e.g. `"RSC-005"`.
    pub id: String,
    /// One-line summary.
    pub title: String,
    /// Why the fix is safe / what the spec says.
    pub rationale: String,
    /// The exact edits it would make.
    pub preview: Vec<Change>,
    /// `true` once this fix has been applied in this session.
    pub applied: bool,
}

/// A snapshot of a session: the starting error count and every proposed fix
/// (each with its current `applied` flag).
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct State {
    pub errors_before: usize,
    pub fixes: Vec<Fix>,
}

/// A repair session over one EPUB, held in WASM memory across calls.
#[wasm_bindgen]
pub struct Session {
    ws: Workspace,
    /// Each planned fix, taken (`None`) once applied.
    fixes: Vec<Option<ProposedFix>>,
    /// Stable, JS-facing descriptions, kept even after a fix is applied.
    infos: Vec<Fix>,
    errors_before: usize,
}

#[wasm_bindgen]
impl Session {
    /// Load an EPUB from its raw bytes, detect its defects, and plan the fixes.
    pub fn load(bytes: &[u8]) -> Result<Session, JsError> {
        let ws = Workspace::load(bytes).map_err(to_js)?;
        let report = ws.detect().map_err(to_js)?;
        let errors_before = report.errors();
        let planned = fixers::plan(&report, &ws, Goal::Valid);
        let infos = planned
            .iter()
            .enumerate()
            .map(|(i, f)| describe(i, f))
            .collect();
        let fixes = planned.into_iter().map(Some).collect();
        Ok(Session {
            ws,
            fixes,
            infos,
            errors_before,
        })
    }

    /// The current session snapshot: the starting error count and the fixes.
    pub fn state(&self) -> State {
        State {
            errors_before: self.errors_before,
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
            self.infos[index].applied = true;
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
                    self.infos[i].applied = true;
                    applied += 1;
                }
            }
        }
        applied
    }

    /// Re-validate the current (possibly repaired) EPUB, returning its error
    /// count — the same independent check the CLI reports as `errors: N → M`.
    pub fn errors_after(&self) -> Result<usize, JsError> {
        Ok(self.ws.detect().map_err(to_js)?.errors())
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
        applied: false,
    }
}

fn to_js(e: epubsana::Error) -> JsError {
    JsError::new(&e.to_string())
}

/// The `epubsana-wasm` crate version.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
