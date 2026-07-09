# Changelog

All notable changes to `epubsana` (and the `epubsana-wasm` bindings, which track
the same version) are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
epubsana is pre-1.0, so breaking changes land as minor-version bumps (`0.x.0`),
per [Cargo's SemVer compatibility
rules](https://doc.rust-lang.org/cargo/reference/semver.html).

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

[0.1.0]: https://github.com/veripublica/epubsana/releases/tag/v0.1.0
