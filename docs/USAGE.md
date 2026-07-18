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

`-i` is the only input form — a positional path is a usage error, so a typo can
never be mistaken for a filename. The original is never modified in place; a
repaired copy is written to `<name>_fixed.epub` (or your `-o`) only if at least
one fix was applied, and an existing output file is never silently replaced
(pass `-f` to allow it).

---

## How it works

epubsana follows a strict five-step contract, the same in every frontend (this
CLI, the in-browser WASM demo, and library consumers such as epublift):

1. **Load** the EPUB into a fidelity-preserving in-memory container. Untouched
   entries round-trip byte-for-byte.
2. **Detect** — run epubveri over the book to get the findings (each with an
   epubcheck-compatible ID, a stable `rule` sub-code, and an exact position).
3. **Propose** — for each finding a fixer can safely handle, build a
   `ProposedFix`: a description, the reason it is safe, and a preview of the
   exact edits. Findings epubsana can't safely fix are left alone.
4. **Confirm** — you decide, per fix, whether to apply it. Nothing mutates
   without your approval (subject to the [policy](#cli-reference) you choose).
5. **Report** — the run ends with a record of what became of every proposed fix
   (applied, skipped, or — in a dry run — merely proposed), the fatal and error
   counts before vs. after, and whether the goal was met.

---

## CLI reference

epubsana conforms to the **[veripublica CLI convention v0.4](https://github.com/veripublica/conventions/blob/main/CLI.md)**,
so its flags, output naming, and exit codes match the other veripublica tools.

```
epubsana -i <PATH> [OPTIONS]
```

| Option | Description |
| --- | --- |
| `-i`, `--input <PATH>` | The input. The only input form; positional paths are not accepted. |
| `-o`, `--output <PATH>` | Where to write the output. Default: `<input-stem>_fixed.epub`, beside the input. Must not be the input. |
| `-f`, `--force` | Permit replacing existing output files. Never lifts the output-equals-input refusal. |
| `--format <FORMAT>` | Report format: `human` (the default) or `json` — the shared machine envelope. |
| `--dry-run` | Report what would happen; change nothing on disk. |
| `-y`, `--yes` | Assume "yes" for every prompt; run non-interactively. Not permission to overwrite files — that is `-f`. |
| `--auto-safe` | Apply the provably-safe fixes without asking; still prompt for the rest. |
| `--goal <valid\|openable>` | How far to repair. Default: `valid`. See [Exit codes](#exit-codes). |
| `-v`, `--verbose` | Show each fix's rationale (why it's safe). |
| `-V`, `--version` | Print `epubsana <version>` and exit `0`. |
| `-h`, `--help` | Print help and exit `0`. |

**Behaviour of the three modes**

- **Default (no flag):** prompt for *every* proposed fix (`[y/N]`).
- **`--auto-safe`:** apply fixes tiered *AutoSafe* automatically; prompt for
  *ConfirmNeeded* fixes. See [tiers](#the-interactive-workflow).
- **`-y`/`--yes`:** approve everything, no prompts. Good for batch/CI use — but
  read [Safety guarantees](#safety-guarantees) first, and prefer `--dry-run` to
  preview.

A prompt epubsana cannot ask is a decision it cannot obtain: when stdin is not a
terminal and fixes would need approval, it **stops** (exit `2`) and names the
flag that would let it proceed, rather than silently assuming "no" and returning
an exit code that looks like an ordinary result.

**`--goal`** does not yet change which fixers run (both goals propose the same
fixes; the distinction will gate fixer selection as the registry grows) — but it
**does** decide what counts as success, and therefore the exit code. See below.

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
APPLIED Map 1 undeclared HTML entity (657×) to characters in OEBPS/Text/bolum2.xhtml (nbsp)
    - replace &nbsp; → ' ' (657×)
SKIPPED Make 55 invalid NCX ids a valid XML NCName in toc.ncx
    - rename NCX id "51100e1e-…" → "id_51100e1e-…"

774 fatal(s), 5 error(s) → 0 fatal(s), 4 error(s)
wrote book_fixed.epub
goal 'valid': NOT MET
```

- Each fix line says what became of it: **APPLIED**, **SKIPPED** (you declined),
  or **WOULD APPLY** (a `--dry-run`). The indented lines are its concrete edits.
- **N fatal(s), N error(s) → …** is epubveri's own count before repair vs. after
  — the book is re-validated at the end, so this is an independent check, not a
  claim.
- **Fatals are counted apart from errors**, exactly as epubveri reports them. A
  fatal is a defect that stops the book from being processed at all (an
  unreadable ZIP, a missing `container.xml`, XHTML that is not well-formed, an
  unterminated entity reference). A book whose defects are all fatal has *zero
  errors* and is not remotely valid — so the fatal count is stated first, and
  always.
- **wrote …** appears only if at least one fix was applied.
- **goal '…': MET / NOT MET** is the line the exit code mirrors.

---

## Exit codes

Per the [convention](https://github.com/veripublica/conventions/blob/main/CLI.md#6-exit-codes),
a transformer's `0` means *the run's goal was met* — and epubsana has two goals:

| Code | Meaning |
| --- | --- |
| `0` | The run's goal was met. With `--goal valid` (the default): no fatal- and no error-severity findings remain — the book is valid. With `--goal openable`: no fatal-severity findings remain — the book opens. |
| `1` | The goal was not met: fixes were declined, or defects epubsana cannot fix remain. |
| `2` | epubsana could not run: a usage error, an unreadable EPUB, `-o` equal to the input, an existing output file without `-f`, an unanswerable prompt, or an I/O failure. |

The default goal is the *verifier's* threshold, so `epubsana -i book.epub -y &&
echo "valid"` means what `epubveri -i book.epub && echo "valid"` means — the two
tools agree by construction.

`--goal openable` is the explicitly-requested **lesser** goal the convention
allows: the e-reader / fix-on-import bar. Under it, **exit `0` can coexist with
errors in the report** — the book opens, which is what the invocation asked. The
errors are still reported; they simply do not move the exit code. The goal is
always printed (and carried in `--format json`'s `summary.goal`), so a `0` is
never read without the bar it was measured against.

---

## Machine output (`--format json`)

`--format json` emits the shared veripublica envelope
([FORMATS.md](https://github.com/veripublica/conventions/blob/main/FORMATS.md)) —
exactly one JSON object on stdout, the same shape epubveri emits, so one parser
reads both:

```json
{
  "tool": "epubsana",
  "tool_version": "0.3.1",
  "convention": "0.4",
  "status": "problems",
  "inputs": [
    {
      "path": "book.epub",
      "status": "problems",
      "output": "book_fixed.epub",
      "summary": {
        "fatals_before": 774, "fatals_after": 0,
        "errors_before": 5, "errors_after": 4,
        "applied": 2, "skipped": 0, "goal": "valid"
      },
      "items": [
        {
          "type": "fix",
          "outcome": "applied",
          "code": "RSC-016",
          "rule": "htm.entity.undeclared",
          "severity": "fatal",
          "location": "OEBPS/Text/bolum2.xhtml",
          "message": "Map 1 undeclared HTML entity (657×) to characters …",
          "data": { "fix_id": "fix.html_entities", "tier": "auto_safe", "changes": ["…"] }
        }
      ]
    }
  ]
}
```

Two fields carry epubsana's half of the contract:

- **`outcome`** — `applied`, `skipped`, or `proposed` — is on **every** fix item.
  A confirm-each-step run routinely applies one fix and declines the next; a
  report that cannot say which is not a report of what changed. Under
  `--dry-run` every item is `"proposed"` (and `dry_run: true` is a summary of
  that, never a contradiction of it).
- **`severity`** is **inherited** from the finding the fix addresses, verbatim
  from epubveri — it describes the *defect*, never epubsana's opinion of its own
  fix. How much judgement the fix needs is a different axis, and lives in
  `data.tier`.

A usage error produces **no envelope**: a short message on stderr and exit `2`.

---

## What epubsana can fix today

Each fixer targets a specific epubveri finding — by epubcheck ID and, where
available, the stable `rule` sub-code — and only proposes an edit when a safe,
content-preserving one exists. For exactly *how* each fix is made, why it's
safe, and when epubsana declines, see the **[fix catalogue](./FIXERS.md)**.

| epubcheck ID | rule sub-code | Tier | What it does |
| --- | --- | --- | --- |
| `RSC-016` | `htm.entity.undeclared` | AutoSafe | Replaces undeclared HTML named entities (`&nbsp;`, `&mdash;`, `&eacute;`, …) used in XHTML without a DTD with the exact character each denotes. Entities it doesn't recognize are left untouched. |
| `RSC-016` | `htm.entity.missing_semicolon` | AutoSafe | Repairs a named entity reference missing its closing `;` (`&nbsp`), which is not well-formed XML. A recognized name becomes the character it denotes; an XML-predefined one (`&amp` …), whose character is the bare delimiter, is closed with `;` instead. Only the unterminated occurrences are touched — a correct `&nbsp;` and a longer entity are left alone; an unrecognized name is declined. |
| `RSC-005` | `ncx.ids.invalid_ncname` | ConfirmNeeded | Makes an invalid NCX `id` a valid XML NCName (e.g. a digit-leading UUID `51100e1e-…` → `id_51100e1e-…`, or a brace-wrapped GUID `{0F57…}` → `id_0F57…`), keeping it unique. Only rewrites an `id` whose attribute is unambiguous. |
| `RSC-005` | `opf.content_document.invalid_content_type_meta` | ConfirmNeeded | Normalizes a content document's encoding declaration to the EPUB 3.3 / HTML5 form: collapses every legacy `<meta http-equiv="Content-Type">` (and any duplicate) into a single `<meta charset="utf-8"/>`. Declines if the document declares a non-UTF-8 charset. |
| `NCX-001` | *(none)* | ConfirmNeeded | Sets the NCX `dtb:uid` to the package's unique identifier (the `dc:identifier` the OPF `unique-identifier` points at), so the two agree. |
| `RSC-005` | `opf.content_document.empty_title` | ConfirmNeeded | Fills an empty `<title></title>` with text **from the book itself**: the label its table of contents gives that document, or failing that the document's own first heading. Declines when the book names the document nowhere — it never invents a title, and never falls back to the book's own `dc:title`. |
| `RSC-020` | `opf.manifest_item.unencoded_space_in_href` | AutoSafe | Percent-encodes a raw space in a manifest `href` (`ch 1.xhtml` → `ch%201.xhtml`). The file keeps its name; only the URL is spelled legally. |
| `OPF-014` | `opf.content_document.property_used_undeclared` | AutoSafe | Adds the property a content document demonstrably uses (`scripted`, `svg`, `remote-resources`, `switch`) to its manifest item's `properties`. The document itself is not touched — the manifest is made to tell the truth about it. |
| `PKG-006` | *(none)* | AutoSafe | Moves the `mimetype` entry to the front of the ZIP, stored uncompressed, as OCF requires. Changes no content at all — not one byte of any entry, `mimetype` included; only where it sits and how it's compressed. Declines if there is no `mimetype` entry to move. |
| `RSC-005` | `htm.epub2_dom.bare_text_in_body` | ConfirmNeeded | Wraps EPUB 2 text sitting directly in `<body>` in a `<div>`, which XHTML 1.1 requires. The text is not altered and the whitespace around it stays put — only a wrapper appears. `<div>` rather than `<p>`: it claims nothing about what the text is, and matches the anonymous block the text already renders as, so the page doesn't move. |
| `RSC-001` | `opf.manifest_item.missing_resource` | ConfirmNeeded | Drops a manifest `<item>` declaring a resource the container doesn't hold — **and, in the same approval, every reference that named it**: the spine `<itemref>`s it would orphan, and a legacy `<meta name="cover">` pointing at it. Nothing readable is lost, because the resource was already gone. Declines if the deletions would empty the `<spine>`. |
| `OPF-049` | `opf.spine.itemref_idref_not_in_manifest` | ConfirmNeeded | Drops a spine `<itemref>` naming a manifest id that doesn't exist — a position no reading system can render, and one nothing in the book says how to repair. Every other entry keeps its place. Declines if it would empty the `<spine>`. |
| `OPF-034` (EPUB 2) / `RSC-005` (EPUB 3) | `opf.spine.duplicate_itemref` | ConfirmNeeded | Keeps the first spine `<itemref>` for a manifest item and drops the later repeats, so a chapter stops appearing twice in the reading order. The kept entry is where the document actually belongs, so nothing moves. Declines when the entries disagree on `linear` (in the reading order *and* reachable out-of-line is deliberate), or when a repeat carries an `id` a `<meta refines>` points at. |
| `HTM-004` | `htm.doctype.epub3_obsolete_public_id` | AutoSafe | An EPUB 3 (HTML5) document may carry only `<!DOCTYPE html>`; any PUBLIC identifier is obsolete, so the DOCTYPE is reduced to that. A doctype declares no content, so nothing a reader sees changes. Declines a DOCTYPE with an internal subset (`[ … ]`), whose entity declarations HTML5 can't carry. |
| `HTM-004` | `htm.doctype.epub2_unrecognized_public_id` | ConfirmNeeded | Canonicalizes an EPUB 2 DOCTYPE whose identifier is a **malformed XHTML 1.1** id (names 1.1, or the `xhtml11.dtd`, but mistypes the exact string) to the recognized form. Declines a document that declares a genuinely different DTD (XHTML 1.0, a bare `<!DOCTYPE html>`, OEB): relabeling it to 1.1 would assert a content model epubsana can't verify and risks trading the finding for content-model errors. |
| `RSC-005` | `ncx.ids.duplicate_id` | ConfirmNeeded | Two or more NCX elements share an `id`. Keeps the first and renames each later duplicate to a unique value. NCX ids aren't referenced by IDREF anywhere in an EPUB, so no reference is rewritten. |
| `RSC-005` | `ncx.play_order.duplicate` | ConfirmNeeded | Navigation elements repeat a `playOrder`. Renumbers every `playOrder` to its 1-based position in document order — the canonical assignment — making the values unique. `playOrder` is only a hint; the real reading order (the spine) is untouched. |
| `RSC-007` | `opf.guide.reference_missing_resource` | ConfirmNeeded | Drops an EPUB 2 `<guide>` reference whose `href` resolves to no resource in the container — a landmark pointing at a hole. If that leaves the `<guide>` empty (invalid, and the element is optional), the `<guide>` is dropped too. Matches on the reported `href`; paths aren't re-resolved. |
| `RSC-017` | `opf.guide.duplicate_reference` | ConfirmNeeded | Two or more `<guide>` references share the same `type` **and** `href`. Keeps the first and drops the redundant repeats. References with the same `type` but different `href` (e.g. several `type="text"`) are not duplicates and are left alone. |

Findings not in this table — dangling links, arbitrary schema violations, and
anything requiring content epubsana would have to invent — are reported by
epubveri but **left untouched**. epubsana never guesses.

Note the shape of the two dangling-reference fixers: they only ever delete a
pointer to something that **isn't there**, which is why deleting loses nothing.
A reference to a resource that *does* exist is never dropped to silence a
finding — that would be destroying content to make a validator happy.

More fixers land in real-world impact order. See
[epubveri](https://github.com/veripublica/epubveri) for the full catalogue of
what can be detected.

---

## Safety guarantees

These invariants hold for every fixer:

- **No mutation without an approved fix.** In the default mode you approve each
  one; `--auto-safe` auto-approves only *AutoSafe* fixes; `--yes` approves all.
- **Surgical and content-preserving.** A fix edits only what it must. An entry
  no fix touched is never decompressed and recompressed — its compressed bytes,
  compression method and timestamp are copied through as-is, and entry order and
  directory entries are kept. (The zip local headers themselves are rebuilt by
  the writer, so a repaired container is not *byte*-identical to its input; every
  byte of every entry's data is.)
- **The container is never quietly normalized.** epubsana does not repackage
  anything as a side effect of writing output — not even a `mimetype` entry that
  violates OCF. If your packaging is wrong, epubveri reports it and it stays
  reported; a defect epubsana did not propose and you did not approve is a defect
  epubsana did not touch.
- **Never guess.** If a finding has no safe, determinate fix, epubsana declines
  it rather than risk the content.
- **Independently re-validated.** After applying fixes, the whole book is
  re-checked with epubveri for the before → after counts — the tool proves its
  own result rather than asserting it.
- **The original isn't modified in place.** Repairs are written to a separate
  output file (by default `<input-stem>_fixed.epub`; overridden only if you
  point `-o` at another path), and an existing file there is never silently
  replaced — epubsana refuses until you pass `-f`.

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

    println!(
        "fatal {} → {}, error {} → {}",
        report.fatals_before, report.fatals_after,
        report.errors_before, report.errors_after,
    );
    for fix in &report.fixes {
        println!("{}: {}", fix.outcome.as_str(), fix.title);
    }

    if report.changed() {
        std::fs::write("book_fixed.epub", ws.serialize()?)?;
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
- **`ChangeReport`** — `fixes` (each a `ReportedFix` carrying its `Outcome`:
  `Applied` / `Skipped` / `Proposed`), `fatals_before`/`fatals_after`,
  `errors_before`/`errors_after`, `goal`, and `goal_met` — the tool's `0`/`1`
  line. `applied()` / `skipped()` / `changed()` are conveniences over `fixes`.
- **`Goal::is_met(&report)`** — `Valid` = no fatals and no errors; `Openable` =
  no fatals (the book opens).
- **`envelope`** — the shared machine shape. The skeleton is epubveri's
  reference type (`epubveri::envelope`, generic over the two tool-owned slots);
  epubsana supplies its own `Summary` and `Data` and maps a `ChangeReport` into
  it, so a library consumer emits exactly the JSON the CLI does — and the family
  keeps one copy of the envelope, not one per tool.
- **`fixers::plan(&report, &ws, goal)`** — build the proposals directly (what
  `--dry-run` uses) without applying anything.

---

## Known limitations

- **Coverage is partial and honest.** epubsana fixes the safely-fixable
  findings and reports the rest; a repaired book is not guaranteed fully valid.
  How much a given library improves varies — the tool clears what it can
  *safely* and leaves the rest reported, untouched.
- **Fixes are planned once, up front.** All proposals are built from the initial
  detection. A structural fixer that can't parse a document *before* an earlier
  fixer would have cleaned it up may decline it. (Re-planning after each fix is
  on the roadmap.)
- **`--goal` decides success, not yet fixer selection.** Both goals propose the
  same fixes today; what differs is the bar the result is measured against (see
  [Exit codes](#exit-codes)).
