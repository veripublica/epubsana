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
apply → report) is solid, with twenty-six fixers so far:

- **`RSC-016`** — undeclared HTML entities (`&nbsp;`, `&mdash;`, …) → the exact
  character each denotes.
- **`RSC-016` / `missing_semicolon`** — a named entity with no closing `;`
  (`&nbsp`) → the character it denotes, or the reference closed. Completes the
  entity family: every entity defect epubveri reports now has a repair.
- **`RSC-005` / `ncx.ids.invalid_ncname`** — invalid NCX ids → valid XML NCNames.
- **`RSC-005` / `invalid_content_type_meta`** — legacy encoding declarations →
  the HTML5 `<meta charset="utf-8">`.
- **`NCX-001`** — NCX `dtb:uid` synced to the package's unique identifier.
- **`RSC-005` / `empty_title`** — an empty `<title>` filled from the book's own
  TOC label (or its first heading); never invented.
- **`RSC-020`** — an unencoded space in a manifest `href` → `%20`.
- **`OPF-014`** — a content property a document demonstrably uses → declared on
  its manifest item.
- **`PKG-006`** — a `mimetype` entry that isn't first in the ZIP → moved to the
  front, stored, with no content touched at all.
- **`RSC-005` / non-block content in `<body>`** — EPUB 2 text *and* inline
  elements (`<a>`, `<br>`, `<img>`, …) sitting directly in `<body>` → each run
  wrapped whole in one `<div>`, so a line that rendered as one block still does;
  the content and the whitespace around it are untouched. An element XHTML 1.1
  doesn't have at all — `<figure>`, `<section>`, `<figcaption>` — ends the run
  and is left alone, because wrapping it would move its violation rather than
  clear it. Non-block content in any other container is declined too: there the
  correct wrapper would assert something about what the content is.
- **`RSC-001`** — a manifest item declaring a resource the container doesn't hold
  → dropped, together with every reference that named it (the spine entries it
  would orphan, and a legacy cover `<meta>`), in one edit you approve once.
- **`OPF-049`** — a spine `itemref` naming a manifest id that doesn't exist →
  dropped. Neither fixer will leave a book with an empty spine; it declines.
- **`OPF-034` / `RSC-005`** — the same manifest item listed twice in the spine
  (a chapter appearing twice) → the first occurrence kept, the repeats dropped.
  Declines when the entries differ in `linear`: that is an authored intent, not
  a duplicate.
- **`HTM-004`** — an obsolete DOCTYPE. An EPUB 3 document's PUBLIC identifier →
  reduced to `<!DOCTYPE html>`; an EPUB 2 document's malformed XHTML 1.1
  identifier → canonicalized. Declines a document that declares a genuinely
  different DTD (XHTML 1.0, …), since relabeling it would assert an unverified
  content model.
- **`RSC-005` / `ncx.ids.duplicate_id`** — duplicate NCX ids → the first kept,
  later ones renamed uniquely (NCX ids aren't reference targets, so nothing else
  moves).
- **`RSC-005` / `ncx.play_order.duplicate`** — repeated NCX `playOrder` values →
  renumbered by document order. Together with the NCName and `dtb:uid` fixers,
  this closes the NCX internal-consistency defects epubsana can determine.
- **`RSC-007` / `opf.guide.reference_missing_resource`** — an EPUB 2 `<guide>`
  reference pointing at a resource that doesn't exist → dropped (and the `<guide>`
  itself if that empties it).
- **`RSC-017` / `opf.guide.duplicate_reference`** — two guide references with the
  same `type` and `href` → the first kept, the duplicate dropped.
- **`RSC-005` / `htm.obsolete_attribute`** — a legacy `<a name="x">` on an element
  that already carries `id="x"` → the `name` dropped. Nothing that linked to the
  anchor moves; the fragment resolves through the `id`. An anchor with no `id`, or
  a different one, is left alone.
- **`RSC-005` / empty `lang`** — an empty `lang=""` / `xml:lang=""`, which EPUB 2
  doesn't allow → deleted, so the element inherits its parent's language. A
  malformed tag is never guessed at.
- **`RSC-005` / an invalid `id`** — an `id` that isn't a valid XML NCName (on our
  shelf, one that starts with a digit) → renamed to the nearest valid, unique
  name, with **every reference moved with it**: fragments in the document, links
  from other documents, the NCX. References are resolved against the referring
  file's own directory rather than replaced globally, so a link meaning another
  document's identically-named id is left alone. Any occurrence it can't
  classify — a fragment in a stylesheet or in prose — makes it decline.
- **`RSC-005` / `opf.package.schema_violation`** — an EPUB 3 attribute on an
  EPUB 2 package → deleted, but only once verified it says nothing the book
  doesn't: a `properties="cover-image"` whose cover is already declared by
  `<meta name="cover">` on that item, or a `page-progression-direction="ltr"`.
  A `properties="nav"`, a cover with no legacy declaration, or an `rtl` reading
  direction are left alone — EPUB 2 has nowhere to put that information, which
  is a reason not to erase it.
- **`RSC-005` / `opf.content_document.duplicate_id`** — two or more elements
  sharing an `id` → the first keeps it, the later ones are renamed uniquely. No
  link moves and none needs to: a `#fragment` already resolves to the first
  element carrying the id, so keeping the first leaves every reference pointing
  exactly where it pointed.
- **`RSC-007` / a stale reference path** — a link like `../Text/notes.xhtml#a8`
  whose target now sits elsewhere in the book → the path is repointed at the one
  container entry carrying that name, with the fragment carried across. Declines
  when the name matches nothing or several entries, when the fragment isn't in
  the target (that would trade one error for a broken link), and for external
  URLs or junk.
- **`OPF-054`** — a `<dc:date>` with no content → dropped; the element states no
  date and `dc:date` is optional. A malformed but non-empty date (`March 2019`)
  is left exactly as it is: it carries a date the author wrote, and deciding
  which characters are stray would be a guess.

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
