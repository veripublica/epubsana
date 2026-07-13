# Changelog

All notable changes to `epubsana` (and the `epubsana-wasm` bindings, which track
the same version) are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
epubsana is pre-1.0, so breaking changes land as minor-version bumps (`0.x.0`),
per [Cargo's SemVer compatibility
rules](https://doc.rust-lang.org/cargo/reference/semver.html).

## [0.2.0] - 2026-07-13

Adopts **[veripublica conventions v0.4](https://github.com/veripublica/conventions)**
and **epubveri 0.5**. Breaking, on purpose: the severity vocabulary grew from
three values to five, and a `fatal` is no longer folded into an `error` — which
changes what "valid" means, and it changes it for the better.

**The trap this release closes:** epubsana's flagship fixer clears *undeclared
HTML entities*, and epubveri 0.5 correctly reports those as **fatal** (a document
that is not well-formed XML does not open). Every count epubsana printed came
from `errors()`, which no longer counts fatals. Left alone, a book with 774 fatal
entity references would have reported `0 error(s)` and been called **valid**.
Fatals are now counted, stated first, and gate the verdict.

### Changed — breaking

- **Positional paths are gone.** `-i/--input` is the only input form; a bare word
  is a usage error that names the flag it should have been (`use -i book.epub`).
  A second `-i` is a usage error too — epubsana is a transformer, and a
  transformer repairs one book at a time, rather than silently keeping the last.
- **`--yes` gained its short form `-y`**, and the argument grammar is now the
  family's, ported from epubveri: `--name=value`, attached `-ivalue`, bundled
  booleans (`-yfv`), POSIX value semantics (`-iv` means `-i v`), and a value
  token that is never re-parsed as an option (`-i -q.epub` names that file).
  A repeated single-valued option (`--format x --format y`) is a usage error:
  two answers to one question, and the tool does not guess.
- **Counts are reported as `N fatal(s), N error(s)`,** fatals first and always.
- **Exit `0` now means "the run's goal was met"** — see below.
- **`ChangeReport` is restructured:** one `fixes` list, each entry carrying its
  `Outcome` (`Applied` / `Skipped` / `Proposed`), plus `fatals_before`/
  `fatals_after`, `goal`, and `goal_met`. (The old `applied`/`skipped` split
  remains available as iterators.)
- **`ProposedFix::addresses_rule` is `Option<&'static str>`** (was
  `Option<String>`): a fixer dispatches on a compile-time rule, and the shared
  envelope's `rule` field is `&'static str`, so it now passes straight through.
- **`epubsana-wasm`:** `Session.state()` → `Session.plan()`, and
  `Session.errors_after()` → `Session.report(goal)`, which re-validates and
  returns the machine envelope's `inputs[i]` shape.

### Added

- **`--goal` now decides what counts as success.** `valid` (the default) is the
  verifier's own threshold — no fatal- and no error-severity findings remain — so
  epubsana's `0` means what epubveri's `0` means, by construction. `openable` is
  the explicitly-requested lesser goal the convention allows: **no fatals
  remain**, the book opens. Under it, exit `0` can coexist with errors in the
  report; the exit code answers the question the invocation asked, and the goal
  is always printed beside the verdict. "No fatals" is not a proxy for openable —
  a fatal *is* the class of defect that stops an EPUB from being processed at all
  (unreadable ZIP, missing `container.xml` or OPF, XHTML that is not well-formed,
  an unterminated entity reference).
- **`--format json`** — the shared veripublica machine envelope
  ([FORMATS.md](https://github.com/veripublica/conventions/blob/main/FORMATS.md)):
  exactly one JSON object on stdout, the same shape epubveri emits. Every `fix`
  item carries **`outcome`** (`applied` / `skipped` / `proposed`) — a
  confirm-each-step run routinely applies one fix and declines the next, and a
  report that cannot say which is not a report of what changed — and a
  **`severity` inherited** from the finding it addresses, never a judgement about
  the fix itself. A usage error produces no envelope.

  The skeleton is **not epubsana's**: `epubsana::envelope` builds on
  [`epubveri::envelope`](https://docs.rs/epubveri)'s reference types (epubveri
  0.5.3, veripublica/epubveri#14), which are generic over the two slots
  FORMATS.md §2 leaves to each tool. epubsana supplies only those two — its
  `summary` vocabulary and its item `data` — so there is exactly **one copy of
  the envelope in the family**, and `Item::fix` makes an item without an
  `outcome` unconstructible.
- **`-f, --force`, and the file-safety rules.** An existing output file is no
  longer silently overwritten: epubsana refuses (exit `2`, naming the path and
  the way through) until `-f` is given. `-f` never lifts the output-equals-input
  refusal, and `-y` is not permission to overwrite files.
- **An unanswerable prompt stops the run.** When stdin is not a terminal and
  fixes need approval, epubsana exits `2` naming `--yes` or `--dry-run`, rather
  than silently assuming "no" and returning an exit code that looks ordinary.
- **`epubsana::VERSION`** — the version with git build metadata
  (`+<short-hash>[.dirty]`), printed identically by `-V`, the json envelope's
  `tool_version`, and the wasm binding's `version()`. A crates.io build (no
  `.git`) falls back silently to the plain SemVer.
- **Browser demo:** adopts the shared
  [family-web](https://github.com/veripublica/family-web) template v3 — theme
  toggle, the family footer nav, and the five-severity colors. Each fix card now
  shows two independent facts side by side: its **tier** (how much judgement it
  needs) and the **severity** of the defect it clears. A goal selector re-checks
  the repaired book against `valid` or `openable`, with epubveri as the judge.

## [0.1.0] - 2026-07-09

Initial release. A pure-Rust EPUB repairer — the fixer half of the
[epubveri](https://github.com/veripublica/epubveri) (detect) → epubsana (repair)
pair. It turns the safely-fixable defects epubveri reports into edits you approve
one at a time, applies the approved ones, and reports exactly what changed. It
never guesses, and it preserves — byte-for-byte — everything it doesn't touch.

### Added

- **Fix contract core** — `Workspace` → detect → propose → confirm → apply →
  report, kept UI-agnostic so every frontend shares one engine and behaviour
  never diverges. Nothing mutates without an approved fix; the run is
  independently re-validated for the before/after counts.
- **Four fixers** (each only proposes an edit when a safe, content-preserving one
  exists; see [`docs/FIXERS.md`](docs/FIXERS.md)):
  - `RSC-016` / `htm.entity.undeclared` — replace undeclared HTML named entities
    (`&nbsp;`, `&mdash;`, …) with the exact character each denotes. *(AutoSafe)*
  - `RSC-005` / `ncx.ids.invalid_ncname` — sanitize an invalid NCX `id` to a
    valid, unique XML NCName. *(ConfirmNeeded)*
  - `RSC-005` / `opf.content_document.invalid_content_type_meta` — normalize a
    content document's encoding declaration to the EPUB 3.3 / HTML5
    `<meta charset="utf-8">`. *(ConfirmNeeded)*
  - `NCX-001` — sync the NCX `dtb:uid` to the package's unique identifier.
    *(ConfirmNeeded)*
- **CLI** conforming to the [veripublica CLI
  convention](https://github.com/veripublica/conventions) v1: `-i/--input`,
  `-o/--output` defaulting to `<name>_fixed.epub` (never in place), `--dry-run`,
  `--yes`, `--auto-safe`, `--goal`, and exit codes `0` (valid) / `1` (errors
  remain) / `2` (could not run).
- **`epubsana-wasm`** — WebAssembly bindings (a stateful `Session` mirroring the
  confirm-each-step contract) and a client-side [demo](https://veripublica.github.io/epubsana/)
  that repairs an EPUB entirely in the browser, with no upload. Published to npm
  as [`@veripublica/epubsana-wasm`](https://www.npmjs.com/package/@veripublica/epubsana-wasm).
- **Docs** — [`docs/USAGE.md`](docs/USAGE.md) (user guide) and
  [`docs/FIXERS.md`](docs/FIXERS.md) (the per-finding fix catalogue: what each
  fix changes, why it's safe, and when it declines).

How much a given library improves varies — epubsana clears what it can *safely*
and leaves the rest reported, untouched.

[0.2.0]: https://github.com/veripublica/epubsana/releases/tag/v0.2.0
[0.1.0]: https://github.com/veripublica/epubsana/releases/tag/v0.1.0
