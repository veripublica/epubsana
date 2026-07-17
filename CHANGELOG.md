# Changelog

All notable changes to `epubsana` (and the `epubsana-wasm` bindings, which track
the same version) are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
epubsana is pre-1.0, so breaking changes land as minor-version bumps (`0.x.0`),
per [Cargo's SemVer compatibility
rules](https://doc.rust-lang.org/cargo/reference/semver.html).

## [Unreleased]

### Added

- **Two fixers for dangling references in the package document**, contributed as
  requirements by `epublift`, which carried its own repair for them
  ([#4](https://github.com/veripublica/epubsana/issues/4),
  [#3](https://github.com/veripublica/epubsana/issues/3)):
  - `RSC-001` / `opf.manifest_item.missing_resource` — a manifest `<item>`
    declaring a resource the container doesn't hold. The declaration is dropped
    **together with every reference that named it**: the spine `<itemref>`s it
    would otherwise orphan, and a legacy `<meta name="cover">` pointing at it.
    Those travel in a single proposal rather than separate ones, because
    approving the item drop and declining the spine drop would leave you with an
    `OPF-049` epubsana created itself.
  - `OPF-049` / `opf.spine.itemref_idref_not_in_manifest` — a spine `<itemref>`
    naming a manifest id that does not exist. Dropped; every other entry keeps
    its place in the reading order.

  Both are `ConfirmNeeded` — they are deletions that can shorten the reading
  order or remove a cover declaration, and epubsana does not delete visible
  structure unattended. Both decline rather than repair when the deletions would
  leave `<spine>` with no children: a spine-less EPUB is not a repaired book.

  On the 171-book corpus this clears every `RSC-001` (3 findings in 2 books) and
  takes both books from invalid to **fully valid** — 26 → 28 books that epubsana
  brings all the way to valid. No book gains a finding.

- **A fixer for a duplicated spine entry** — the same manifest item listed twice,
  so a chapter appears twice in the reading order
  ([#2](https://github.com/veripublica/epubsana/issues/2)). The first occurrence
  is kept and the repeats dropped: the repeat carries no information the first
  doesn't, and the first is where the document belongs in the sequence.

  epubveri reports this condition under **two ids** — `OPF-034` in EPUB 2,
  `RSC-005` in EPUB 3 — with one shared `rule`, so the fixer keys on the `rule`
  and inherits the id from the finding. A fixer written against `OPF-034` alone
  would have done nothing on every EPUB 3 book.

  Declines rather than guessing when the duplicate's `linear` disagrees with the
  first's (the book means "in the reading order *and* reachable out-of-line",
  which is deliberate), or when a repeat carries an `id` that a `<meta refines>`
  targets. Not present in the reference corpus, which contains no Kindle→EPUB
  conversions — it lands on `epublift`'s reproduction of it in the wild.

### Changed

- **Track `epubveri` 0.5.12** (from 0.5.9). No source change — the `rule`/`params`
  contract held across the bump; the effect is behavioural, driven by two upstream
  fixes we reported and measured:
  - [epubveri#23](https://github.com/veripublica/epubveri/issues/23): EPUB 2
    documents with DTD-declared entities (`&nbsp;` under an XHTML 1.1 DOCTYPE) now
    parse, so a class of false `RSC-012` "fragment not defined" findings is gone.
    On the corpus, `RSC-012` drops from 1247 to 172 — the 172 are the genuinely
    dangling fragments, the ~1075 removed were the detector failing to read a valid
    document and calling its ids absent. `empty_title` findings rise +157 as the
    same documents become readable.
  - [epubveri#25](https://github.com/veripublica/epubveri/issues/25): a regression
    in 0.5.10/0.5.11 that turned any EPUB 2 document with a `[` in its body (a
    footnote marker) into a false fatal. The corpus had 78 such false fatals across
    11 books on 0.5.11; 0.5.12 has zero.

  Net on the 171-book corpus: 28 → **30** books brought all the way to valid, still
  zero regressions. The manifest floor is now `epubveri = "0.5.12"` — 0.5.9 is wrong
  86% of the time on `RSC-012`, and 0.5.10/0.5.11 carry the #25 false fatal, so
  building against any of them is not acceptable.

## [0.4.0] - 2026-07-16

Two new fixers, and the writer stops quietly rewriting your container.

**Why this is `0.4.0` and not `0.3.3`:** `serialize()`'s output changes for every
book. Entries are no longer decompressed and recompressed, so a repaired file's
bytes differ from what 0.3.x produced, and packaging — including a `mimetype`
entry that violates OCF — is now preserved rather than normalized on the way out.
Anything downstream that relied on writing output to quietly correct packaging
must now approve `fix.mimetype_packaging` instead. No API was removed.

### Added

- **A ninth fixer: `RSC-005` / `htm.epub2_dom.bare_text_in_body`**
  (`fix.bare_text_in_body`, ConfirmNeeded). Wraps text sitting directly inside an
  EPUB 2 `<body>` — which XHTML 1.1 forbids, since it wants block-level content
  there — in a `<div>`, one proposal per document. The text itself is not
  altered, and the wrapper goes around its non-whitespace span only, so the
  document's existing line breaks and indentation stay exactly where they were.

  `<div>` rather than `<p>` on purpose: it claims nothing about what the text
  *is* (in the corpus it is chapter titles and converter leftovers alike), and it
  reproduces the anonymous block a reading system already lays bare text out in,
  so nothing moves on the page. That choice of default is what makes this
  ConfirmNeeded rather than AutoSafe.

  **Whitespace-only text nodes are never wrapped.** They are the line breaks
  between sibling elements, epubveri does not report them, and across the six
  affected corpus books `<body>` holds **7,594** of them against **54** real
  ones — a fixer that wrapped them all would add thousands of empty `<div>`s per
  book. Corpus, every fix approved: 12 proposals over 6 books clear all 54
  findings, **5 more books become fully valid** (21 → 26), no regressions.

- **An eighth fixer: `PKG-006` — `mimetype` is not the first entry**
  (`fix.mimetype_packaging`, AutoSafe). Re-emits the `mimetype` entry first and
  stored uncompressed, as OCF requires so a reading system can identify the file
  from its opening bytes. It is the first fixer that touches **no content at
  all** — not one byte of any entry, `mimetype` included; only that entry's
  position and compression method change, and OCF allows exactly one answer for
  each. Declines when there is no `mimetype` entry to move: inventing one would
  assert what the file *is* rather than repair how it is packaged. Dispatches on
  the bare `id` (like `NCX-001`), which `PKG-006` can carry alone — it says one
  thing and its subject is the container itself, so nothing needs
  disambiguating.

  This is the repair the writer used to perform invisibly (see below). The
  round-trip is now honest end to end: on the corpus the same **2 books of 171**
  are repaired as before, but as a proposal you can see, approve, or decline.

### Fixed

- **Untouched entries are no longer decompressed and recompressed.** The writer
  rebuilt every entry from scratch, so writing any output re-deflated the whole
  container: measured across a 171-book corpus, **not one book** survived a
  no-op load-and-write unchanged — 166 grew, 13 had entries silently switch
  compression method, and `META-INF/` directory entries were dropped outright.
  The original archive is now retained and any entry a fix did not rewrite is
  raw-copied: same compressed bytes, method, timestamp, order, directories
  included. An entry a fix *does* rewrite keeps the compression method the
  original used, rather than defaulting to deflate.
- **The container is no longer normalized behind your back.** `serialize()`
  always re-emitted `mimetype` first and stored, which repaired `PKG-005` /
  `PKG-007` as a side effect of writing *any* output — with no fix item, no
  proposal and no approval. That directly contradicted the crate's own "no
  mutation without an approved fix" guarantee. Packaging is now preserved
  exactly as it arrived; a book whose `mimetype` violates OCF keeps saying so
  until a fix proposes otherwise. On the corpus this affects **2 books of 171**
  (the other 169 already package `mimetype` correctly), whose real packaging
  defect epubveri now reports instead of epubsana quietly laundering it.

### Changed

- **`docs/USAGE.md`'s safety guarantees now state what is actually true.** They
  claimed "every other byte of the container round-trips unchanged" — false for
  every book measured. A repaired container is *not* byte-identical to its
  input: the zip writer derives local headers rather than copying them (the
  version-needed field and general-purpose hint bits come out as its own, ~180
  bytes per book). Every byte of every entry's *data* is preserved, which is the
  guarantee that was meant, and nothing semantic is lost — bit 11, the UTF-8
  entry-name flag, is re-derived from the name.

## [0.3.2] - 2026-07-16

Tracks `epubveri` 0.5.9. No epubsana source changed — the fixers key on the
stable `rule` contract, which held — but the upstream detection fix removes a
whole class of proposal epubsana should never have made, so it ships as its own
release. Re-audited on the 171-book corpus with every fix approved: **no
regressions** (no finding appears that was not there before), errors 4078 →
1206, 21 books become fully valid.

### Changed

- **`epubveri` 0.5.9 → `content_type_meta` no longer fires on EPUB 2.** Upstream
  fixed a false positive: the rule requiring `<meta http-equiv="Content-Type">`
  to read exactly `text/html; charset=utf-8` is an HTML5 rule and applies to
  **EPUB 3** only. EPUB 2 content is XHTML 1.1, where
  `content="application/xhtml+xml; charset=utf-8"` is the correct form. Because
  epubsana's `content_type_meta` fixer keys on that finding, it was proposing to
  rewrite those valid EPUB 2 declarations into the HTML5
  `<meta charset="utf-8"/>` form — a form XHTML 1.1 does not want. Those
  proposals are now gone: on the 171-book corpus the fixer drops from **18 books
  / 845 proposals to zero**, and books reporting errors fall from 128 to 125.
  Every activation it had was a false positive. The fixer is unchanged and still
  correct for EPUB 3; this corpus simply contains no EPUB 3 book that needs it.
  Repair burden correctly removed, not lost coverage.
- **`RSC-011` findings now anchor at the source `<a>` element** rather than the
  OPF package root, and carry a `data.element_path` in JSON. epubsana has no
  `RSC-011` fixer today, so there is no behavior change — but a future one would
  have been blocked by the old location, the same way `OPF-073` still is.

## [0.3.1] - 2026-07-15

Tracks `epubveri` 0.5.8. No epubsana source changed — the fixers key on the
stable `rule`/`params` contract, which held — but one upstream detection fix is
user-visible, so it ships as its own release.

### Changed

- **`epubveri` 0.5.7 → 0.5.8.** Two upstream detection changes flow through
  without any epubsana code change:
  - epubveri no longer reports the ~250 DTD-declared HTML named entities
    (`&nbsp;`, `&eacute;`, `&copy;`, …) as undeclared (`RSC-016`) in **EPUB 2**,
    where the DOCTYPE's DTD does declare them — it was a false positive. epubsana
    therefore stops proposing to convert those entities in EPUB 2 books: they
    were never broken, so this is repair burden correctly removed, not lost
    coverage. Genuinely undeclared entities still report, and **EPUB 3** (which
    wants numeric character references) is unchanged. Expect fewer
    `html_entities` proposals on EPUB 2 books.
  - `RSC-005` content-model findings now carry the offending element's name in
    `params` (previously empty). No behavior change here — epubsana's `RSC-005`
    consumer keys on the NCX-NCName rule, not the content-model one — but the
    forthcoming content-model fixer gets the element name for free.

## [0.3.0] - 2026-07-15

Adds three corpus-chosen fixers and realigns the foundation to the epubveri
family (edition 2024, `zip` 8.x, `roxmltree` 0.21).

### Added — three fixers, chosen from a census of the real corpus

- **`RSC-005` / `opf.content_document.empty_title`** *(ConfirmNeeded)* — fills an
  empty `<title></title>`. This is the **most widespread defect in the corpus**:
  more books carry it than carry undeclared entities. The text is never invented
  — it is the label the book's **own table of contents** gives that document
  (NCX `navLabel`, or the nav document's `<a>` text), or failing that the
  document's **own first heading**. When the book names the document nowhere, the
  fixer declines and the finding stays reported; it deliberately does *not* fall
  back to the book's `dc:title`, because stamping the book's name onto every
  chapter is a guess about intent, not a repair.
- **`RSC-020` / `opf.manifest_item.unencoded_space_in_href`** *(AutoSafe)* —
  percent-encodes a raw space in a manifest `href`. The file keeps its name; only
  the URL is spelled legally, and `%20` resolves to the very same entry.
- **`OPF-014` / `opf.content_document.property_used_undeclared`** *(AutoSafe)* —
  declares a property the content document demonstrably uses (`scripted`, `svg`,
  `remote-resources`, `switch`) on its manifest item. epubveri proved the usage,
  so the declaration is not a guess: the manifest is made to tell the truth about
  a document that is not itself modified.

### Changed — foundation aligned with the epubveri family

- **`epubveri` 0.5.3 → 0.5.7**, which is itself edition-2024 / MSRV-1.88, so the
  crate follows: **edition 2021 → 2024** and **`rust-version = 1.88`**. No source
  change was needed to compile on the new edition.
- **`zip` 2.x → 8.6** (a `zlib-rs` deflate backend). The family shares one `zip`
  major because epubsana re-emits the containers epubveri reads. **Repaired
  files' bytes change as a result** — same content, and output is still
  byte-for-byte deterministic.
- **`roxmltree` 0.20 → 0.21**, which now matches attributes by local name and
  ignores namespace. A local `NodeExt::attr_no_ns` restores the exact,
  no-namespace lookups the fixers rely on, so `attribute("id")` never also
  matches `xml:id`.

### Verified

Across the corpus (171 books), applying every proposed fix introduces **no new
defect**, and the proposal set is **byte-identical between the 0.5.3 and 0.5.7
stacks** — the foundation bump changed nothing about what epubsana proposes or
applies. Some findings do appear afterwards that were not reported before: they
are *unmasked*, not caused — a document that was not well-formed could not be
schema-checked at all, so clearing its entities lets epubveri see, for the first
time, defects that were always there. Each traces to a file that was fatal before
the repair.

The known plan-once ceiling is measured: fixes are planned from the original
report, so a defect that only becomes visible *after* an earlier fix is not
proposed in the same run. A second pass proposes further fixes across a handful of
books (though it changes no book's overall verdict).

## [0.2.1] - 2026-07-13

### Fixed

- **The `epubveri` version requirement was too low, and 0.2.0 shipped with it.**
  `Cargo.toml` declared `epubveri = "0.5"` while the code needs **0.5.3** — the
  release where the envelope types became reusable (`Envelope::for_tool`,
  `Item::fix`). Against 0.5.0–0.5.2 epubsana does not compile, so a consumer
  whose lockfile held an earlier 0.5.x got a compile error out of a released
  crate. The requirement is now `epubveri = "0.5.3"`.

  Nothing else changed: same behaviour, same API, same output. If your build of
  0.2.0 worked, it resolved epubveri to 0.5.3 already.

### Changed

- CI now builds against the **declared minimum** epubveri, so the promise a
  version requirement makes — *"this crate builds against anything from here
  up"* — is checked rather than assumed. That is the bug above, caught by a
  machine next time.

## [0.2.0] - 2026-07-13

Adopts **[veripublica conventions v0.4](https://github.com/veripublica/conventions)**
and **epubveri 0.5.3**. Breaking, on purpose: the severity vocabulary grew from
three values to five, and a `fatal` is no longer folded into an `error` — which
changes what "valid" means, and it changes it for the better.

**The trap this release closes:** epubsana's flagship fixer clears *undeclared
HTML entities*, and epubveri 0.5.3 correctly reports those as **fatal** (a document
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

[0.2.1]: https://github.com/veripublica/epubsana/releases/tag/v0.2.1
[0.2.0]: https://github.com/veripublica/epubsana/releases/tag/v0.2.0
[0.1.0]: https://github.com/veripublica/epubsana/releases/tag/v0.1.0
