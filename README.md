# epubsana

**Repairs the EPUB defects [epubveri](https://github.com/veripublica/epubveri)
detects** — a fast, pure-Rust, JVM-free EPUB fixer.

epubveri *finds* what's wrong in an EPUB (with epubcheck-compatible message
IDs and exact positions); **epubsana** turns the safely-fixable findings into
**edits you approve one at a time**, applies them, and reports **exactly what
changed**. It never guesses, and it preserves everything it doesn't touch.

> Part of the **veripublica** family: `epubveri` (verify) + `epubsana` (heal).

## Status

Early but working. The core contract (`Workspace` → detect → propose → confirm →
apply → report) is solid, with four fixers so far:

- **`RSC-016`** — undeclared HTML entities (`&nbsp;`, `&mdash;`, …) → the exact
  character each denotes.
- **`RSC-005` / `ncx.ids.invalid_ncname`** — invalid NCX ids → valid XML NCNames.
- **`RSC-005` / `invalid_content_type_meta`** — legacy encoding declarations →
  the HTML5 `<meta charset="utf-8">`.
- **`NCX-001`** — NCX `dtb:uid` synced to the package's unique identifier.

Measured on a 171-book corpus, they clear ~79% of all errors with zero
regressions. More fixers land next, in real-world impact order.

See **[docs/USAGE.md](docs/USAGE.md)** for the full guide — CLI reference, the
confirm-each-step workflow, the fixer catalogue, exit codes, and library usage.

## Usage

```sh
# See what would be fixed, change nothing:
epubsana -i book.epub --dry-run

# Repair, confirming each fix, writing book_fixed.epub:
epubsana -i book.epub

# Apply every proposed fix without prompting:
epubsana -i book.epub --yes -o repaired.epub
```

The CLI conforms to the [veripublica conventions](https://github.com/veripublica/conventions)
(`-i`/`-o`, `<name>_fixed.epub` output, exit `0`/`1`/`2`), so it behaves like the
other veripublica tools. Full guide: **[docs/USAGE.md](docs/USAGE.md)**.

## Design

Every frontend (this CLI, a future in-browser WASM page, and
[epublift](https://github.com/ePubLift/epublift) integration) shares one core
contract so behavior never diverges: fixes are proposed as data, the caller
decides per fix (`Confirmer`), and the run ends with a `ChangeReport`. Nothing
mutates without an approved fix.

## License

Dual-licensed: **AGPL-3.0-only** OR a **commercial license** — see
[`LICENSE`](./LICENSE) and [`LICENSE-COMMERCIAL.md`](./LICENSE-COMMERCIAL.md).
