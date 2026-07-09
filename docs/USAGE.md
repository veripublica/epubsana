# epubsana — Usage Guide

**epubsana repairs the EPUB defects [epubveri](https://github.com/veripublica/epubveri)
detects.** It turns the safely-fixable findings into edits you approve one at a
time, applies the approved ones, and reports exactly what changed. It never
guesses, and it preserves — byte-for-byte — everything it doesn't touch.

- **In scope:** *repair* — surgical, content-preserving fixes that clear
  validation errors while keeping the book otherwise identical.
- **Out of scope:** *modernization* (upgrading EPUB 2 → 3.3, rebuilding the
  table of contents, fetching metadata, archiving). That is
  [epublift](https://github.com/ePubLift/epublift)'s job. epubsana deliberately
  serves publishers who still ship EPUB 2 for older readers, so it fixes a book
  *in place* without changing its version.

> This guide is kept in step with the tool. When a fixer is added or changed,
> update [What epubsana can fix today](#what-epubsana-can-fix-today) and, if the
> CLI changes, [CLI reference](#cli-reference). See
> [Maintaining this document](#maintaining-this-document).

---

## Table of contents

- [Install](#install)
- [Quick start](#quick-start)
- [How it works](#how-it-works)
- [CLI reference](#cli-reference)
- [The interactive workflow](#the-interactive-workflow)
- [The repair report](#the-repair-report)
- [Exit codes](#exit-codes)
- [What epubsana can fix today](#what-epubsana-can-fix-today)
- [Safety guarantees](#safety-guarantees)
- [Reference standard](#reference-standard)
- [Using epubsana as a Rust library](#using-epubsana-as-a-rust-library)
- [Known limitations](#known-limitations)
- [Maintaining this document](#maintaining-this-document)

---

## Install

epubsana is pure Rust with no C dependencies.

**The CLI** — from [crates.io](https://crates.io/crates/epubsana):

```sh
cargo install epubsana
```

**In the browser** — no install at all: repair an EPUB with the
[in-browser demo](https://veripublica.github.io/epubsana/) (your file never
leaves the page). For a JS project, add the WASM bindings from npm:

```sh
npm install @veripublica/epubsana-wasm
```

**From source** (optional):

```sh
git clone https://github.com/veripublica/epubsana
cd epubsana
cargo install --path .
```

---

## Quick start

```sh
# 1. See what would be fixed — changes nothing:
epubsana -i book.epub --dry-run

# 2. Repair interactively, approving each fix (writes book_fixed.epub):
epubsana -i book.epub

# 3. Apply every proposed fix without prompting, to a chosen path:
epubsana -i book.epub --yes -o repaired.epub
```

A positional path also works (`epubsana book.epub`). The original file is never
modified in place; a repaired copy is written to `<name>_fixed.epub` (or your
`-o`) only if at least one fix was applied.

---

## How it works

epubsana follows a strict five-step contract, the same in every frontend (this
CLI, a future in-browser WASM page, and library consumers such as epublift):

1. **Load** the EPUB into a fidelity-preserving in-memory container. Untouched
   entries round-trip byte-for-byte.
2. **Detect** — run epubveri over the book to get the findings (each with an
   epubcheck-compatible ID, a stable `rule` sub-code, and an exact position).
3. **Propose** — for each finding a fixer can safely handle, build a
   `ProposedFix`: a description, the reason it is safe, and a preview of the
   exact edits. Findings epubsana can't safely fix are left alone.
4. **Confirm** — you decide, per fix, whether to apply it. Nothing mutates
   without your approval (subject to the [policy](#cli-reference) you choose).
5. **Report** — the run ends with a record of every fix applied, every fix
   skipped, and the error count before vs. after.

---

## CLI reference

epubsana conforms to the **[veripublica CLI convention v1](https://github.com/veripublica/conventions/blob/main/CLI.md)**,
so its flags, output naming, and exit codes match the other veripublica tools.

```
epubsana -i <book.epub> [OPTIONS]
epubsana <book.epub> [OPTIONS]
```

| Option | Description |
| --- | --- |
| `-i`, `--input <path>` | The EPUB to repair. A positional path also works. |
| `-o`, `--output <path>` | Where to write the repaired EPUB. Default: `<name>_fixed.epub`, next to the input. Must not be the input. |
| `--dry-run` | Show the fixes that would be proposed and change nothing. |
| `--yes` | Apply every proposed fix without prompting. |
| `--auto-safe` | Auto-apply provably-safe fixes; prompt for the rest. |
| `--goal <openable\|valid>` | How far to repair. Default: `valid`. |
| `-V`, `--version` | Print version and exit. |
| `-h`, `--help` | Print help and exit. |

**Behaviour of the three modes**

- **Default (no flag):** prompt for *every* proposed fix (`[y/N]`).
- **`--auto-safe`:** apply fixes tiered *AutoSafe* automatically; prompt for
  *ConfirmNeeded* fixes. See [tiers](#the-interactive-workflow).
- **`--yes`:** approve everything, no prompts. Good for batch/CI use — but read
  [Safety guarantees](#safety-guarantees) first, and prefer `--dry-run` to
  preview.

**`--goal`** is accepted today but does not yet change which fixers run
(`openable` is the "at least it opens in an e-reader" bar; `valid` targets full
epubcheck validity). The distinction will gate fixer selection as the registry
grows; for now both propose the same fixes.

---

## The interactive workflow

In the default and `--auto-safe` modes, epubsana prints each proposed fix and
asks before applying it:

```
[ConfirmNeeded] Make 55 invalid NCX ids a valid XML NCName in toc.ncx
    - rename NCX id "51100e1e-b21d-4d41-…" → "id_51100e1e-b21d-4d41-…"
    - rename NCX id "36d9b249-ecd7-4ebe-…" → "id_36d9b249-ecd7-4ebe-…"
    …
  Apply this fix? [y/N]
```

- Type `y` (or `Y`) to apply; anything else — including just Enter — skips it.
- The `[Tier]` prefix tells you how much intervention the fix needs:
  - **`AutoSafe`** — exactly one correct, content-preserving fix; safe to apply
    unattended. `--auto-safe` applies these without asking.
  - **`ConfirmNeeded`** — a good fix that makes a visible change (e.g. renaming
    an id, rewriting an encoding declaration); you should look before approving.
- The indented lines are the **preview**: the exact edits this fix would make.

---

## The repair report

Every run (except `--dry-run`) ends with a report:

```
— repair report —
APPLIED Make 55 invalid NCX ids a valid XML NCName in toc.ncx
    - rename NCX id "51100e1e-…" → "id_51100e1e-…"
    …
APPLIED Normalize the encoding declaration in chapter1.xhtml to HTML5 <meta charset="utf-8">
    - normalize to a single <meta charset="utf-8"/> (1 encoding <meta> rewritten/removed)
Skipped 0 fix(es).
errors: 150 → 0
wrote book_fixed.epub
```

- **APPLIED** blocks list every fix that was applied and its concrete edits.
- **Skipped** counts the fixes you declined (or that `--dry-run` left).
- **errors: N → M** is epubveri's error count before repair vs. after — the
  book is re-validated at the end, so this is an independent check, not a claim.
- **wrote …** appears only if at least one fix was applied.

---

## Exit codes

Per the [convention](https://github.com/veripublica/conventions/blob/main/CLI.md#6-exit-codes):

| Code | Meaning |
| --- | --- |
| `0` | The book is valid after repair (or was already). |
| `1` | Repair ran, but some errors remain (epubsana cleared what it safely could). |
| `2` | The tool could not run (bad arguments, unreadable/corrupt EPUB, `-o` equal to the input, I/O failure). |

This lets a script branch on the result: `epubsana -i book.epub --yes && echo "fully valid"`.
The `errors: N → M` line shows the same thing in human form. In `--dry-run`, the
code reflects the book's *current* state (`0` clean, `1` has errors).

---

## What epubsana can fix today

Each fixer targets a specific epubveri finding — by epubcheck ID and, where
available, the stable `rule` sub-code — and only proposes an edit when a safe,
content-preserving one exists. For exactly *how* each fix is made, why it's
safe, and when epubsana declines, see the **[fix catalogue](./FIXERS.md)**.

| epubcheck ID | rule sub-code | Tier | What it does |
| --- | --- | --- | --- |
| `RSC-016` | `htm.entity.undeclared` | AutoSafe | Replaces undeclared HTML named entities (`&nbsp;`, `&mdash;`, `&eacute;`, …) used in XHTML without a DTD with the exact character each denotes. Entities it doesn't recognize are left untouched. |
| `RSC-005` | `ncx.ids.invalid_ncname` | ConfirmNeeded | Makes an invalid NCX `id` a valid XML NCName (e.g. a digit-leading UUID `51100e1e-…` → `id_51100e1e-…`, or a brace-wrapped GUID `{0F57…}` → `id_0F57…`), keeping it unique. Only rewrites an `id` whose attribute is unambiguous. |
| `RSC-005` | `opf.content_document.invalid_content_type_meta` | ConfirmNeeded | Normalizes a content document's encoding declaration to the EPUB 3.3 / HTML5 form: collapses every legacy `<meta http-equiv="Content-Type">` (and any duplicate) into a single `<meta charset="utf-8"/>`. Declines if the document declares a non-UTF-8 charset. |
| `NCX-001` | *(none)* | ConfirmNeeded | Sets the NCX `dtb:uid` to the package's unique identifier (the `dc:identifier` the OPF `unique-identifier` points at), so the two agree. |

Findings not in this table — missing resources, dangling links, arbitrary schema
violations, and anything requiring content epubsana would have to invent — are
reported by epubveri but **left untouched**. epubsana never guesses.

More fixers land in real-world impact order. See
[epubveri](https://github.com/veripublica/epubveri) for the full catalogue of
what can be detected.

---

## Safety guarantees

These invariants hold for every fixer:

- **No mutation without an approved fix.** In the default mode you approve each
  one; `--auto-safe` auto-approves only *AutoSafe* fixes; `--yes` approves all.
- **Surgical and content-preserving.** A fix edits only what it must; every
  other byte of the container round-trips unchanged.
- **Never guess.** If a finding has no safe, determinate fix, epubsana declines
  it rather than risk the content.
- **Independently re-validated.** After applying fixes, the whole book is
  re-checked with epubveri for the `errors: N → M` count — the tool proves its
  own result rather than asserting it.
- **The original isn't modified in place.** Repairs are written to a separate
  output file (by default `<name>.fixed.epub`; overridden only if you point
  `-o` at another path).

---

## Reference standard

epubsana repairs toward **EPUB 3.3** (the current W3C Recommendation) and the
latest epubcheck rules. When both EPUB 2 and EPUB 3 forms would be valid, it
emits the most-current one — e.g. an encoding declaration becomes the HTML5
`<meta charset="utf-8">`, not the legacy `<meta http-equiv="Content-Type">`.

It does **not** rewrite legacy *features* wholesale (it won't drop the NCX for a
navigation document, for instance) — that is modernization, and belongs to
epublift. epubsana makes the legacy artifact *valid*, in place.

---

## Using epubsana as a Rust library

Every frontend shares one core crate, so a library consumer (epublift, your own
tool) gets identical behavior. The `Confirmer` trait is how "confirm each step"
lives in the core: you decide, per fix, whether to apply it.

```rust
use epubsana::{repair, Confirmer, Decision, Goal, Policy, ProposedFix, Workspace};

// A confirmer that approves everything (like the CLI's `--yes`). A real UI
// would inspect `fix.title`, `fix.rationale`, and `fix.preview` and ask a human.
struct ApproveAll;
impl Confirmer for ApproveAll {
    fn decide(&mut self, _fix: &ProposedFix) -> Decision {
        Decision::Approve
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read("book.epub")?;
    let mut ws = Workspace::load(&bytes)?;

    let mut confirmer = ApproveAll;
    let report = repair(&mut ws, Goal::Valid, Policy::AskEach, &mut confirmer)?;

    println!("errors: {} → {}", report.errors_before, report.errors_after);
    for applied in &report.applied {
        println!("applied: {}", applied.title);
    }

    if !report.applied.is_empty() {
        std::fs::write("book.fixed.epub", ws.serialize()?)?;
    }
    Ok(())
}
```

Key types:

- **`Workspace`** — the mutable, fidelity-preserving EPUB. `load(bytes)` reads,
  `serialize()` writes it back, `detect()` runs epubveri over the current state.
- **`repair(ws, goal, policy, confirmer)`** — the whole contract in one call:
  detect → propose → confirm (per `policy`) → apply → return a `ChangeReport`.
- **`Policy`** — `AskEach` (ask the confirmer for every fix),
  `AutoSafeThenAsk` (auto-apply *AutoSafe*, ask for the rest), or `DryRun`
  (propose and report, apply nothing).
- **`Confirmer`** — implement `decide(&mut self, fix: &ProposedFix) -> Decision`
  (`Approve` / `Reject`).
- **`ChangeReport`** — `applied`, `skipped`, `errors_before`, `errors_after`.
- **`fixers::plan(&report, &ws, goal)`** — build the proposals directly (what
  `--dry-run` uses) without applying anything.

---

## Known limitations

- **Coverage is partial and honest.** epubsana fixes the safely-fixable
  findings and reports the rest; a repaired book is not guaranteed fully valid.
  It clears a large share of real-world errors with zero regressions — measured
  on a 171-book corpus, the current fixers clear ~79% of all errors (18,348 →
  3,904) and bring 22 of the 145 invalid books (~15%) to fully valid — but your
  mileage varies by book.
- **Fixes are planned once, up front.** All proposals are built from the initial
  detection. A structural fixer that can't parse a document *before* an earlier
  fixer would have cleaned it up may decline it. (Re-planning after each fix is
  on the roadmap.)
- **`--goal` doesn't gate fixers yet** (see [CLI reference](#cli-reference)).

---

## Maintaining this document

When you add or change a fixer:

1. Add/update its row in
   [What epubsana can fix today](#what-epubsana-can-fix-today) — ID, `rule`,
   tier, and a one-line description.
2. Add a section to the [fix catalogue](./FIXERS.md) using its template —
   **Finding**, **Fix**, **Why it's safe**, **When it declines** — plus a
   Summary row. This is the spec reviewers check the code against.
3. If it introduces a new CLI flag or changes behaviour, update
   [CLI reference](#cli-reference).
4. If measured coverage changes materially, update the figure in
   [Known limitations](#known-limitations) (keep it honest — cite the corpus).

Keep this in sync with `README.md`'s short status blurb.
```
