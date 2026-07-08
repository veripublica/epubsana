# epubsana

**Repairs the EPUB defects [epubveri](https://github.com/veripublica/epubveri)
detects** — a fast, pure-Rust, JVM-free EPUB fixer.

epubveri *finds* what's wrong in an EPUB (with epubcheck-compatible message
IDs and exact positions); **epubsana** turns the safely-fixable findings into
**edits you approve one at a time**, applies them, and reports **exactly what
changed**. It never guesses, and it preserves everything it doesn't touch.

> Part of the **veripublica** family: `epubveri` (verify) + `epubsana` (heal).

## Status

Early bootstrap. The core contract (`Workspace` → detect → propose → confirm →
apply → report) works, with the first and highest-impact fixer:

- **`RSC-016` — undeclared HTML entities** (`&nbsp;`, `&mdash;`, `&eacute;`, …
  used in XHTML without a DTD): replaced with the exact character each denotes.

More fixers (invalid NCX ids, media-type mismatches, content properties, OCF
packaging, metadata, …) land next, in real-world impact order.

## Usage

```sh
# See what would be fixed, change nothing:
epubsana book.epub --dry-run

# Repair, confirming each fix, writing book.fixed.epub:
epubsana book.epub

# Apply every proposed fix without prompting:
epubsana book.epub --yes -o repaired.epub
```

## Design

Every frontend (this CLI, a future in-browser WASM page, and
[epublift](https://github.com/ePubLift/epublift) integration) shares one core
contract so behavior never diverges: fixes are proposed as data, the caller
decides per fix (`Confirmer`), and the run ends with a `ChangeReport`. Nothing
mutates without an approved fix.

## License

Dual-licensed: **AGPL-3.0-only** OR a **commercial license** — see
[`LICENSE`](./LICENSE) and [`LICENSE-COMMERCIAL.md`](./LICENSE-COMMERCIAL.md).
