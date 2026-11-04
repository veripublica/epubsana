# epubsana: detail for code agents and maintainers

`README.md` is the short introduction. This file holds the detail and is kept
true at every release. Each fixer's exact edit and its declines are in
[`docs/FIXERS.md`](docs/FIXERS.md), and the walk-through for people, with the
`--apply` workflow and full JSON examples, is in
[`docs/USAGE.md`](docs/USAGE.md). Past changes: `CHANGELOG.md`. API docs:
<https://docs.rs/epubsana>.

Figures below name the versions they were measured on.

## Scope

epubsana repairs EPUB files. It is the fixer in a chain: epubveri detects,
epubsana repairs, epublift modernizes. It turns the findings of
[epubveri](https://github.com/veripublica/epubveri) that are safe to fix into
edits a person or a program approves one at a time, applies them, checks the
result and reports what changed.

- **A fixer exists only where there is exactly one correct, content-preserving
  repair.** Everything else is reported and left alone. Reaching 100% of findings
  is not the aim.
- **epubsana repairs what epubveri reports, and nothing else.** It is not a second
  validator. A finding that looks wrong is a question for epubveri, not a local
  guard here. Fixers dispatch on the finding's id, its stable `rule` key and its
  `params`, never on message text.
- **Where a parser recovers the same way every time, the repair writes down what
  readers already render,** not what the author may have meant.
- **The specs decide:** EPUB 3.3, CSS and XHTML, with epubcheck parity as the
  floor. Where EPUB 2 and EPUB 3 both allow a form, a fix emits the current one,
  such as `<meta charset="utf-8">`.
- **Every frontend calls the one core `repair()`:** the CLI, the WebAssembly
  package and epublift. The same book with the same approvals comes back byte for
  byte the same from each.

There are 55 fixers. Three act on findings below error severity
(`opf.metadata.empty_element`, `opf.manifest_item.non_preferred_media_type`,
`opf.guide.duplicate_reference`), so they clear findings without moving the error
count.

### How a run works

`Workspace::load` → `detect` (epubveri) → `fixers::plan` → the caller approves or
rejects each fix → apply → `detect` again → report.

If the count of any `(id, rule)` pair rose after applying, the journal is bisected
to find the fix responsible, that fix is marked `reverted`, and the rest are
replayed. A rise under a fix that lowered the fatal count, or that cleared a rule
which stops the validator seeing the book, is accepted as *revealed*: those
findings were always there. The report gives the count, and running again can
repair some of them.

While nothing has been edited, `detect` validates the original bytes. After edits
it validates the re-serialized container.

Some fixers ask the detector before proposing: `fix.svg_attribute_case` applies its
renames to a copy of the book and leaves out what epubveri still reports, and
`fix.svg_element_case` also leaves out every rename in a document where any other
`(id, rule)` count rises. A new fixer should reuse this rather than keep its own
table of what is allowed.

## What it declines to do

- **Guess.** If choosing a value means choosing what the author meant, it declines:
  two candidate identifiers, `lang` and `xml:lang` carrying different real values,
  a reference path matching several files, a fragment defined in several documents.
- **Invent content.** `<title>` is filled only from the book's own TOC label, first
  heading, or `dc:title`, in that order.
- **Modernize.** It keeps the NCX and other legacy features, and makes them valid
  in place. Upgrades are epublift's job.
- **Normalize the container on the way through.** A defect it did not propose and
  you did not approve is left as it was, even a misplaced `mimetype`.
- **Delete an NCX entry whose target is absent from the book.** Where exactly one
  file carries that name, the path is repointed; otherwise the entry stays.
- **Repair malformed CSS or invalid CSS values.** It handles three CSS findings:
  `css.font_face.missing_target`, `css.stylesheet.unterminated` and
  `css.url.missing_resource`. epubveri's advisory CSS checks (`--advisory`, such as
  ADV-024) are not requested, reported or repaired.
- **Remove `page-map` from an EPUB 3 spine,** at present. Removing it would lose the
  page numbers Adobe readers show.
- **Edit a book.** Some findings are an editor's job. epubsana is an engine, with no
  editor and no library database.

## Install

Pre-built binaries are attached to every
[GitHub Release](https://github.com/veripublica/epubsana/releases/latest), for macOS
(Intel and Apple Silicon), Windows (x64 and ARM) and Linux (x64 and ARM, each as a
fully static `musl` build plus a dynamically linked `gnu` variant). They need no
toolchain. The browser version is at <https://veripublica.github.io/epubsana/>.

```sh
cargo install --locked epubsana         # the CLI
npm install @veripublica/epubsana-wasm  # WASM bindings for the browser
```

`--locked` builds with the exact dependency versions the release was tested
against. On Windows, Rust also needs Microsoft's linker, which comes with
[Build Tools for Visual Studio](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
when you tick *Desktop development with C++*.

## CLI

Conforms to the [veripublica CLI convention v0.7](https://github.com/veripublica/conventions/blob/main/CLI.md).

```
epubsana -i <PATH> [OPTIONS]
```

| Option | Meaning |
| --- | --- |
| `-i`, `--input <PATH>` | The input. Positional paths are not accepted. |
| `-o`, `--output <PATH>` | Output path. Default `<input-stem>_fixed.epub` beside the input. Must not be the input. |
| `-f`, `--force` | Permit replacing an existing output file. Never lifts the output-equals-input refusal. |
| `--format <FORMAT>` | `human` (default) or `json`. |
| `--dry-run` | Report what would happen; change nothing on disk. |
| `-y`, `--yes` | Approve every fix, no prompts. Not permission to overwrite; that is `-f`. |
| `--auto-safe` | Apply `AutoSafe` fixes without asking; prompt for the rest. |
| `--apply <LIST>` | Apply exactly the listed fixes and ask nothing. Comma-separated 1-based plan indices and/or fix ids. For a plugin or script that has already asked a person. |
| `--goal <valid\|openable>` | The bar the exit code measures. Default `valid`. |
| `-v`, `--verbose` | Show each fix's rationale. |
| `-V`, `--version` | Print the version and exit `0`. |
| `-h`, `--help` | Print help and exit `0`. |

With no mode flag, every fix prompts `[y/N]`. Both goals propose the same fixes
today; the goal changes only what counts as success.

### Exit codes

| Code | Meaning |
| --- | --- |
| `0` | The goal was met. `valid`: no fatal or error findings remain. `openable`: no fatal findings remain, even if errors do. |
| `1` | The goal was not met: fixes were declined, or defects epubsana cannot fix remain. |
| `2` | It could not run: usage error, unreadable EPUB, `-o` equal to the input, existing output without `-f`, unanswerable prompt, or an I/O failure. |

Severity has five values, and a fatal is not an error. `ChangeReport` keeps
`fatals_*` and `errors_*` apart, so read both.

### JSON report

`--format json` prints one object on stdout, in the shared envelope
([FORMATS.md](https://github.com/veripublica/conventions/blob/main/FORMATS.md)) that
epubveri also emits. Top level: `tool`, `tool_version`, `convention`, `status`,
`inputs`.

Each input has `path`, `status`, `output`, `summary` and `items`, or `error` when
the run failed.

- **`summary`:** `*_before` and `*_after` for fatals, errors, warnings, infos and
  usages; `applied`, `skipped`, `proposed`, `reverted`; `goal`; and `dry_run: true`
  on a dry run.
- **Each item:** `type: "fix"`, `outcome`, `code`, `rule`, `severity`, `location`,
  `message`, and `data` with `index`, `fix_id`, `tier` and `changes` (`path`,
  `note`).

Outcomes are `applied`, `skipped`, `proposed` and `reverted`. Tiers are `auto_safe`
(one correct, content-preserving fix, safe unattended) and `confirm_needed` (a good
fix that makes a visible change). Full examples are in `docs/USAGE.md`.

## API

### Rust

```rust
use epubsana::{repair, Confirmer, Decision, Goal, Policy, ProposedFix, Workspace};

struct ApproveAll;
impl Confirmer for ApproveAll {
    fn decide(&mut self, _fix: &ProposedFix) -> Decision { Decision::Approve }
}

let mut ws = Workspace::load(&bytes)?;
let report = repair(&mut ws, Goal::Valid, Policy::AskEach, &mut ApproveAll)?;
if report.changed() {
    std::fs::write("book_fixed.epub", ws.serialize()?)?;
}
```

- **`Workspace`:** `load`, `serialize`, `detect`.
- **`repair(ws, goal, policy, confirmer)`:** the whole contract, returning a
  `ChangeReport`.
- **`Policy`:** `AskEach`, `AutoSafeThenAsk`, `DryRun`.
- **`ChangeReport`:** `fixes` (each a `ReportedFix` with an `Outcome`),
  `fatals_before`/`after`, `errors_before`/`after`, `goal`, `goal_met`, and
  `revealed`.
- **`fixers::plan(&report, &ws, goal)`:** the proposals, without applying anything.
- **`envelope`:** maps a `ChangeReport` to the JSON the CLI emits.

epubveri's types are in this API, so moving its floor is always a minor release.

### WebAssembly

`@veripublica/epubsana-wasm` exposes `Session` and `version()`. `Session.load(bytes)`
reads a book, and `plan()` returns the proposals. `repair(approved, goal)` runs the
core `repair()` on the listed plan indices, always from the book as loaded, so
calling it again replaces the previous run. After that, read `revealed()`,
`report(goal)` and `result_bytes()`. See
[`epubsana-wasm/README.md`](epubsana-wasm/README.md).

## Limits

- **Inflated size:** a book with an entry past 64 MiB, or past 256 MiB in total, is
  refused whole (`MAX_ENTRY_BYTES`, `MAX_BOOK_BYTES`). The largest book in my test
  library inflates to 86.5 MB.
- **XML:** every parse first goes through epubveri's `xmlguard`: depth 256, 256
  attributes per element, 1,000,000 elements, and entity expansion up to the entry
  limit. A document past any of them is left untouched and gets no proposal.
- **Planned once.** Proposals are built from the first detection, so a fixer that
  cannot parse a document before an earlier fix cleans it up declines that document
  until the next run. Running again on the output can help.
- **The container writer** (`src/zipwrite.rs`) copies untouched entries verbatim:
  both headers, data descriptor and order, with only the offset patched. Rewritten
  entries keep their header fields; CRC and sizes follow the new data. Bytes outside
  every entry are not written. `zip` is used for reading only.
- **MSRV** is Rust 1.88. Pure Rust, with no C dependencies.

## Measurements

From a private library of real books, one person's, so a number here says nothing
about books unlike those. A zero means only that this library holds no such case.

| Date, version | Books | Result |
| --- | --- | --- |
| 2026-10-28, 0.36.0 | 562 | Written with no fix applied, each book comes back byte-identical. |
| 2026-11-04, 0.37.0 → 0.38.0 | 562 | `(id, rule)` counts before and after, plan, outcomes and repaired bytes: no difference. No hostile file was tried. |
| 2026-11-04, 0.38.0 → 0.39.0 | 562 | Every fix approved: exactly 3 books change, each through `fix.email_as_http_url`, with one fewer error and no fix reverted. |

Before a release, `scripts/preflight.sh` runs everything CI runs. At 0.39.0,
`cargo test --workspace` passes 542 tests. `examples/fuzz.rs` corrupts two defective
seed books and runs each result through `repair()` with every fix approved, looking
for panics.

## Layout

| Path | What |
| --- | --- |
| `src/lib.rs` | Core types and `repair()`: journal, bisect, revert. |
| `src/fixers.rs` | Every fixer, `plan()`, and their tests. |
| `src/workspace.rs` | The book in memory: reading, edits, `detect`. |
| `src/zipwrite.rs` | The container writer. |
| `src/envelope.rs` | The JSON envelope. |
| `src/entities.rs` | HTML named character references to their characters. |
| `src/main.rs` | The CLI. |
| `epubsana-wasm/` | WebAssembly bindings and the demo page. |
| `docs/FIXERS.md` | The specification of each fixer. Written before its code. |
| `docs/USAGE.md` | The guide for people. |
| `examples/fuzz.rs` | The mutation fuzzer. |
| `scripts/` | `bump.sh`, `preflight.sh`, `wait-runs.sh`, `release-verify.sh`. |

## Build and test

```sh
cargo build --release
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo fmt --all --check
cargo build -p epubsana-wasm --target wasm32-unknown-unknown --locked
wasm-pack build epubsana-wasm --target web
cargo run --release --example fuzz
```

To add a fixer:

1. Spec it in `docs/FIXERS.md` first.
2. Dispatch on `rule` and `params`.
3. Give every guard a test that fails when the guard is removed.
4. Check it on real books, diffing per book, because a total can hide a traded
   defect.

There must be one `styloria`, one `epubveri` and one `zip` in the lock. Preflight
fails otherwise.

## Contributing and licence

`AGPL-3.0-only OR LicenseRef-veripublica-Commercial`; `LICENSING-FAQ.md` explains
which applies to what. The books you repair are yours. External contributions are
not accepted yet, because the dual licence requires one copyright holder
(`CONTRIBUTING.md`). Security reports: `SECURITY.md`.
