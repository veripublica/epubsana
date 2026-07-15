# epubsana

**Repairs the EPUB defects [epubveri](https://github.com/veripublica/epubveri)
detects** — a fast, pure-Rust EPUB fixer.

epubveri *finds* what's wrong in an EPUB (with epubcheck-compatible message
IDs and exact positions); **epubsana** turns the safely-fixable findings into
**edits you approve one at a time**, applies them, and reports **exactly what
changed**. It never guesses, and it preserves everything it doesn't touch.

> Part of the **veripublica** family: `epubveri` (verify) + `epubsana` (heal).

## Status

Early but working. The core contract (`Workspace` → detect → propose → confirm →
apply → report) is solid, with seven fixers so far:

- **`RSC-016`** — undeclared HTML entities (`&nbsp;`, `&mdash;`, …) → the exact
  character each denotes.
- **`RSC-005` / `ncx.ids.invalid_ncname`** — invalid NCX ids → valid XML NCNames.
- **`RSC-005` / `invalid_content_type_meta`** — legacy encoding declarations →
  the HTML5 `<meta charset="utf-8">`.
- **`NCX-001`** — NCX `dtb:uid` synced to the package's unique identifier.
- **`RSC-005` / `empty_title`** — an empty `<title>` filled from the book's own
  TOC label (or its first heading); never invented.
- **`RSC-020`** — an unencoded space in a manifest `href` → `%20`.
- **`OPF-014`** — a content property a document demonstrably uses → declared on
  its manifest item.

More fixers land next, in real-world impact order.

See **[docs/USAGE.md](docs/USAGE.md)** for the full guide — CLI reference, the
confirm-each-step workflow, the fixer catalogue, exit codes, and library usage.

## Install

```sh
cargo install epubsana                  # the CLI (crates.io)
npm install @veripublica/epubsana-wasm  # WASM bindings for the browser
```

Or repair a book right in your browser — no install, no upload, your file never
leaves the page: **https://veripublica.github.io/epubsana/**

## Usage

```sh
# See what would be fixed, change nothing:
epubsana -i book.epub --dry-run

# Repair, confirming each fix, writing book_fixed.epub:
epubsana -i book.epub

# Apply every proposed fix without prompting:
epubsana -i book.epub --yes -o repaired.epub

# Machine-readable report (the shared veripublica envelope):
epubsana -i book.epub --format json --dry-run
```

The CLI conforms to the [veripublica conventions
v0.4](https://github.com/veripublica/conventions) (`-i`/`-o`/`-f`,
`<input-stem>_fixed.epub` output, `--format json`, exit `0`/`1`/`2`), so it
behaves like the other veripublica tools. Full guide:
**[docs/USAGE.md](docs/USAGE.md)**.

**Two goals, two questions.** `--goal valid` (the default) asks *"is the book
valid?"* — exit `0` when no fatal- and no error-severity findings remain, the
same line epubveri draws. `--goal openable` asks the e-reader's question — *"does
it open?"* — and exits `0` when no **fatal** findings remain, even if errors do.
The exit code answers the question the invocation asked, and the goal is always
printed alongside it.

## Design

Every frontend (this CLI, the [in-browser WASM demo](https://veripublica.github.io/epubsana/), and
[epublift](https://github.com/ePubLift/epublift) integration) shares one core
contract so behavior never diverges: fixes are proposed as data, the caller
decides per fix (`Confirmer`), and the run ends with a `ChangeReport`. Nothing
mutates without an approved fix.

## License

Dual-licensed: **AGPL-3.0-only** OR a **commercial license** — see
[`LICENSE`](./LICENSE) and [`LICENSE-COMMERCIAL.md`](./LICENSE-COMMERCIAL.md).
