# Changelog

All notable changes to `epubsana` (and the `epubsana-wasm` bindings, which track
the same version) are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
epubsana is pre-1.0, so breaking changes land as minor-version bumps (`0.x.0`),
per [Cargo's SemVer compatibility
rules](https://doc.rust-lang.org/cargo/reference/semver.html).

## [Unreleased]

### Added

- **A fixer for a package whose declared identifier points at nothing usable**
  (`opf.package.unique_identifier_unresolved` + `opf.package.opf_identifier_not_empty`).
  Two rules, one defect at two stages, hitting disjoint sets of five books each:
  either no `<dc:identifier>` carries the id `unique-identifier` names, or the one
  that does is empty. The declared id is attached to the book's single real
  identifier, and any leftover empty element is dropped.

  **It invents nothing.** The value was already in the book, written by its
  producer; the id was already in the package, written in the attribute. The
  repair only attaches one to the other — the same principle as `empty_title`
  moving a TOC label the author wrote.

  **The whole-shelf audit earned its keep here.** The first cut cleared `OPF-030`
  and produced **three new `NCX-001`s**: making the package identifier resolvable
  is what first lets epubveri compare the NCX's `dtb:uid` against it, so the
  repair *unmasked* a pre-existing mismatch. No unit test could have caught that —
  the edit was correct and the book ended up worse. The `dtb:uid` now syncs in the
  same proposal, on the pattern `fix.manifest_dangling_item` already set.

  **Measured: 10 findings across 10 books, 3 repaired and 7 declined** — and the
  declines are the point. Four books carry both a UUID and an ISBN with no `id` on
  either, where which identity is canonical is an editorial decision; two carry no
  `<dc:identifier>` at all, where the repair would have to generate one. With the
  NCX syncs the run clears 8 findings and introduces nothing.

### Changed

- **`fix.ncx_play_order` now repairs all three `playOrder` faults, and no longer
  risks creating one.** epubveri reports `duplicate` (different targets sharing a
  number), `target_mismatch` (one target reached by different numbers) and `gap`
  separately, and they interlock — satisfying one naively breaks another. The
  fixer now reassigns the whole NCX as the format defines: 1-based, dense, in
  document order, with elements naming the same target sharing the first number
  that target was given.

  The previous version numbered by position in the file — unique and dense, but
  **target-blind**, so on a book whose navigation reaches one position by two
  routes it would have *created* `target_mismatch`. No shelf book had that shape,
  so no audit could have shown it; the defect surfaced from reading epubveri's own
  rule, which skips a repeated number when all its holders name the same target.
  A corpus cannot find that class of defect; the detector's source can.

  The renumbering now **parses** the NCX rather than scanning it for
  `playOrder=`, since a target cannot be read off a string — so an NCX that will
  not parse is declined rather than rewritten, consistent with the other
  structural fixers.

  **Measured:** 8 `target_mismatch` and 1 `gap` cleared on top of the 14
  `duplicate`, nothing introduced, and **one more book reaches fully valid**.

- **The body-level wrapper now also works inside `<blockquote>` — the single
  largest lever this project has found.** XHTML 1.1 requires block content in
  `<blockquote>` exactly as it does in `<body>`, and the 10 EPUB 2 books added on
  2026-08-08 carry 2,508 stray-text and 3,009 incomplete-content findings there.

  **The two messages are one defect seen from opposite ends**, which is why both
  now trigger the fixer: a `<blockquote>` holding only text is *stray text is not
  allowed directly in "blockquote"* **and** *element "blockquote" has incomplete
  content*, because its model needs at least one block child. One wrap clears
  whichever fired — verified on real books before a line was written: one file
  went `{incomplete 53, stray 54, span 1}` → `{}`, another
  `{incomplete 96, span 98, img 7}` → `{img 7}`, the inline elements clearing as
  part of the run.

  **The container set is now a principle rather than a name:** the containers
  whose XHTML 1.1 model requires block content and admits `<div>` — `body` and
  `blockquote`. `<ol>`/`<ul>` want an `<li>` and `<head>` a `<title>`, wrappers
  that assert what the content *is*, so they stay declined. On the 125-book shelf
  that rule is not a compromise but the whole population: stray text is reported
  in exactly those two containers and nowhere else.

  **Measured:** findings cleared through `schema_violation` go **1,273 →
  8,624**, total cleared 1,745 → **9,096**, books touched 25 → 26, and both
  whole-shelf instruments still report **nothing introduced anywhere**.

  Honest ceiling: this does *not* clear all 3,009 incomplete-content findings,
  any more than it clears all 403 on `<body>` (it clears 2 of those). The rest
  are containers whose only children are `<figure>`/`<section>` — elements XHTML
  1.1 does not have — where the repair would be renaming, which epubsana
  deliberately does not do.

- **Tracks `epubveri` 0.9.14.** Zero source change; `styloria` 0.8 → 0.9 comes
  along transitively. Upstream's own figure verified from here: the only
  `(id, rule)` pair that moved is `CSS-008 / css.stylesheet.invalid_selector`,
  **0 → 21** in one book, and the shelf run confirms it passes straight through —
  errors before **13087 → 13108** and after **3991 → 4012**, the same +21 on both
  sides, because no fixer of ours consumes it. Everything else identical, zero
  regressions.

  Two upstream items worth recording even though they move nothing here:

  - **`RSC-010` gained its fallback clause**, the defect diagnosed from this side
    on 2026-08-08 (a nav/NCX link to a non-Content-Document is legal when the
    manifest declares a fallback chain reaching one). No shelf book has the
    shape; the IDPF `haruko-jpeg` sample that prompted it loses its three errors.
  - **epubcheck's JSON and XML reports cap identical messages at 25**
    (`CheckMessage.java`, `MAX_LOCATIONS`), with the "N additional locations"
    line commented out — so a consumer sees a truncated list with no indication
    anything was dropped. epubsana runs no second oracle, so nothing here is
    affected, but any cross-tool count taken from epubcheck's JSON is
    incomparable above 25 per message. Recorded so it is never quoted by
    accident.

- **Tracks `epubveri` 0.9.12** (from 0.9.9). Zero source change again; the shelf
  grew to **125 books** the same day. Re-measured: errors **6376 → 4631** over
  the 25 books epubsana touches, **5** books reach fully valid, **nothing
  introduced** by either whole-shelf instrument. Every fixer's proposal count is
  unchanged, which is the answer to upstream's heads-up: 0.9.12 adds **171**
  `element "img" is missing a required attribute` findings across 10 books, and
  none of them reaches a fixer of ours.

- **`handled_rules()` now lists `ncx.uid.package_identifier_mismatch` and
  `ocf.mimetype.not_first_entry`.** Both sites were rule-less when their fixers
  were written, so those fixers dispatch on the bare `NCX-001` / `PKG-006` id;
  epubveri 0.9.11 named them. The dispatch is unchanged — our floor is 0.9.7,
  where the slugs do not exist — but a census reading this list would otherwise
  have filed two rules we *do* fix under "no fixer at all", which is exactly the
  mislabelling the list exists to prevent.

### Not built, deliberately

- **`<img>` with no `alt` (171 findings / 10 books, the widest-spread shape on
  the shelf) is a decline, and upstream predicted it would be.** XHTML 1.1 makes
  `alt` required where HTML5 does not, so this is one of the few places EPUB 2 is
  the stricter version. There is no determinate repair: `alt=""` is correct for a
  decorative image and wrong for a meaningful one, and nothing in the finding —
  or in the book — says which. Supplying either would be inventing content, which
  is the line `empty_title` already draws. Recorded in `docs/COVERAGE.md` rather
  than left to be re-derived.

## [0.8.0] - 2026-08-07

Two fixers, twenty-five to **twenty-six**, and the epubveri 0.9.9 bump. Measured
on the shared shelf — which grew to **115 books** the same week — errors fall
**6220 → 4475** over the 25 books epubsana touches, **5** books reach fully
valid, and both whole-shelf instruments report **nothing introduced anywhere**.

The release's own lesson is in the second fixer: `reference_missing_resource` was
investigated on 2026-08-06, found to be nine unrepairable bare hostnames, and
written off as human-only. The shelf grew, the new books carried a different
shape entirely, and a fixer shipped for it a day later with the detector
unchanged. **A rule closed as unrepairable is closed for that corpus, not for
good** — recorded in `docs/COVERAGE.md` with the habit it argues for.


### Added

- **A fixer for a reference whose path is stale but whose target is still in the
  book** (`opf.content_document.reference_missing_resource`) — and the reason it
  exists is that **a rule closed as unrepairable is not closed for good.** On
  2026-08-06 all nine findings on the 94-book shelf were scheme-less bare
  hostnames or placeholder junk, so this rule was written off as human-only. The
  shelf grew to 115 books and brought a different shape entirely: a real internal
  link left behind by a restructured book —

      1/Bolum013.xhtml links to  ../Text/DiPNOTLAR.xhtml#a8
      the file actually lives at 1/DiPNOTLAR.xhtml

  The path portion is repointed at the entry that carries the name, expressed
  relative to the referring document, and the **fragment is carried across
  untouched**. It is determinate because exactly one container entry has that
  basename, so the target is not chosen from a set.

  **The fragment does double duty.** Repointing could have traded this finding for
  a dangling `RSC-012`, so the fragment must already exist in the chosen target
  before anything is proposed — and that check is also evidence, since a
  same-named file that merely happened to be elsewhere would not carry `#a8`. All
  24 repairable cases pass it, and the shelf run introduces no `RSC-012`.

  Declined: a basename matching nothing (the file is genuinely absent) or several
  entries, an external URL, a bare hostname, placeholder junk, a percent-encoded
  path, and a reference not visible as a quoted attribute value.

  **Measured:** 36 findings across 8 books, of which **24 in 3 books** are the
  repairable shape and clear completely.

- **A fixer for a duplicate `id` in a content document**
  (`opf.content_document.duplicate_id`). The first occurrence in document order
  keeps the id; every later one is renamed to a unique value.

  **No reference is rewritten, and the reason is the fixer's whole argument.**
  Content-document ids *are* reference targets — unlike the NCX ids of the
  sibling fixer — so "rename and touch nothing else" needed justifying rather
  than asserting. It holds because a fragment into a document with a duplicated
  id already resolves to the **first** element in tree order carrying it, which
  is what every conforming processor does and what a reader has been seeing.
  Keeping the first therefore leaves every `#fragment` pointing at exactly the
  element it already pointed at; renaming the first and moving references would
  be the riskier repair for no gain.

  Disjoint from the invalid-id fixer by construction: that one renames an id it
  can prove occurs exactly once and declines a duplicated one, because which
  element a reference meant would then be a guess. Where a value is both
  duplicated and not a valid NCName, the new names are built from the sanitized
  stem, so the repair cannot manufacture more invalid names than it found.

  **Measured:** 53 findings across 21 distinct ids in **one** book, ids repeating
  two to four times each, and **none of them referenced anywhere in its book** —
  so the reasoning carries this fixer, not the corpus. All 53 clear; both
  whole-shelf instruments report nothing introduced.

### Changed

- **Tracks `epubveri` 0.9.9** (from 0.9.7). Two releases, **zero source change**,
  all tests green — the `rule`/`params` contract held again. Re-measured on the
  shared shelf, which grew the same week: **115 books** (was 94), errors
  **6220 → 4552** over the 25 books epubsana touches, **5** books taken from
  invalid to fully valid, **1,668** findings cleared, and **nothing introduced**
  by either whole-shelf instrument. Two fixers fired on real books for the first
  time — `fix.manifest_dangling_item` and `fix.mimetype_packaging` — because the
  new books exercise them, not because anything changed here.

  The dependency floor stays at `0.9.7`, per the tracking policy: it is a
  *correctness* floor, and nothing in 0.9.8/0.9.9 makes an older detector cause
  epubsana to repair something wrongly. The caret picks the newer one up anyway.

  What moved upstream, and what it meant here:

  - **epubveri #66 landed in full, ARIA included.** EPUB 2 documents no longer
    accept 195 global attributes — event handlers, RDFa, microdata, ITS, and now
    `role` plus all 47 `aria-*`. This is the tightening epubsana cited on
    2026-08-06 as one of three reasons **not** to build an
    HTML5-element-downgrade fixer. The forecast is now fact: that surface would
    today include stripping ARIA from books where it was retrofitted for
    accessibility. The decision stands, and is recorded with its evidence in
    `docs/COVERAGE.md`.
  - **New lettered message IDs** (`OPF-004a`–`f`, `OPF-007a`–`c`, `RSC-006b`,
    `RSC-007w`, `HTM_060a`/`b`). Audited: **every rule epubsana consumes still
    maps to exactly one id on the shelf**, so no fixer publishes a wrong code.
    Worth keeping in view, because 23 of 24 fixers assert their `addresses_id`
    rather than inheriting it from the finding — correct today, and the reason
    the spine-duplicate fixer is the one exception is that its condition really
    does arrive under two ids.
  - **`RSC-026` now fires on any reference that escapes the container root**, not
    only manifest hrefs, and is *additive* with the missing-resource rules. On the
    shelf that is one book, 8 findings, all `css.font_face.leaks_container_root`
    — a stylesheet at the container root asking for `url(../Fonts/…)`. No fixer
    of ours is keyed on it and none collides with it.
  - Smaller upstream corrections that change what a repairer sees but ask nothing
    of it: an obsolete attribute is no longer reported twice, `RSC-025` no longer
    fires on EPUB 2 at all, an empty `dc:identifier` no longer cascades into three
    extra findings, and `epub:trigger` is accepted.

## [0.7.0] - 2026-08-06

Four fixers' worth of new repair — three added and one widened, twenty-one to
**twenty-four** — chosen by measuring the shelf rather than by working down a
list. On the 94 shared books, error-severity findings fall **4677 → 3366**, one
more book reaches fully valid, and both whole-shelf instruments (`regression_audit`
and the round-trip check, which compares ID sets as well as counts) report
**nothing introduced anywhere**.

Two of them are the first of their kind here: one edits more than a single file,
and one deletes on an `AutoSafe` tier because it verifies the redundancy first.
As much of the work went into what these decline — a malformed date, an
identically-named id in another document, an element XHTML 1.1 does not have —
and each entry below says so, because that is the part a repairer is judged on.

This is also the first release the tag-triggered automation publishes end to end,
and it carries the `LICENSE-COMMERCIAL.md` packaging fix that npm 0.6.0 shipped
without.


### Changed

- **`fix.bare_text_in_body` now owns all non-block content in `<body>`, not just
  text.** XHTML 1.1 wants block-level content there, and a converter leaves inline
  *elements* behind as readily as text: the 94-book shelf holds 281 stray `<a>`,
  92 `<br>`, and a handful of `<img>`/`<span>`/`<sup>`/`<sub>` sitting directly in
  `<body>`, reported as `element "a" is not allowed here; expected one of …
  "div" …`. The detector's own `params` name `div` at that position, so the
  wrapper this fixer already used is the one the grammar asks for.

  It wraps each **maximal run** — text and inline elements together, in document
  order — in a single `<div>`. That grouping is the point rather than a detail:
  of 244 runs on the shelf, **116 mix text and elements**, so wrapping text alone
  would have split one rendered line into two blocks, and two fixers each owning
  half a run would have collided, the second silently finding nothing to do while
  the report claimed it applied.

  **What it refuses is the larger half.** `figure` (151 findings), `section`
  (92), `figcaption` (90), `center` and a stray `li` arrive through the same
  message at the same position, and a `<div>` around one would *not* clear it —
  XHTML 1.1 does not have the element at all, so the violation would move rather
  than go away. Repairing those means renaming them, which is a different
  operation and a different argument. They end the run and are left alone.

  The `fix_id` is unchanged even though the name is now narrower than the
  behaviour: it is a published identifier `epublift` reads out of our JSON.

  **Measured:** findings cleared on the shelf rise by **380**, books receiving at
  least one proposal go 15 → **18**, and both whole-shelf instruments still report
  nothing introduced anywhere.

### Added

- **A fixer for an EPUB 3 attribute on an EPUB 2 package document**
  (`opf.package.schema_violation`) — the third of the family, and the one whose
  value is in what it refuses to do. All four findings on the shelf turn out to
  assert nothing the book does not already say, so each is deleted **only after
  that redundancy is verified in that book**: a `properties="cover-image"` whose
  cover is also declared by `<meta name="cover">` on the same item, and a
  `page-progression-direction="ltr"`, which is the default everywhere.

  Any other `properties` token (`nav`, `mathml`, …) declines — EPUB 2 has no
  equivalent declaration, so dropping one would discard a real claim rather than
  a repeat. So does a `cover-image` with no matching or a mismatched
  `<meta name="cover">`, where the attribute is the *only* cover declaration, and
  so does `page-progression-direction="rtl"`: a right-to-left reading order is
  authored information EPUB 2 has nowhere to put, which is a reason to leave the
  book alone rather than erase it.

  `AutoSafe`, unusually for a deletion, because the redundancy is checked before
  the fix is proposed rather than assumed from the shape.

  **Measured:** 4 findings across 4 books, all cleared — the rule goes to zero on
  the shelf, with nothing introduced. The declines cost nothing here because no
  shelf book carries the shapes they guard against; they are written from the
  specification, not from the corpus.

- **A fixer for an `id` that is not a valid XML NCName** — and the first one that
  edits more than one file at a time. All 312 findings on the 94-book shelf are
  one defect: an id that starts with a digit. Each is sanitized to the nearest
  valid, unique name, exactly as the NCX fixer does.

  **The reason it took a cross-file design** is that unlike NCX ids, these are
  reference targets: 191 of the 312 are pointed at, 181 times from the NCX, 150
  from other content documents. So every reference moves with the id it names —
  fragments inside the document, links from other documents, the NCX's
  `<content src="…#…"/>` — in the same edit you approve once. A rename that left
  a reference behind would trade this finding for a dangling fragment.

  References are **resolved**, never globally replaced: the path part of the
  attribute value is resolved against the referring file's own directory, and
  rewritten only when it lands on this document. That is not caution for its own
  sake — six values on the shelf are carried by 6–12 *different documents of the
  same book*, so a global rewrite would move links meaning another document's
  identically-named id.

  Anything that cannot be classified — a `#value` in a stylesheet selector, in
  script, or in prose — makes the fixer decline that id rather than guess.

  **Measured:** 312 findings across 5 books, and after the repair all five hold
  **zero** invalid ids. One more book reaches fully valid, and both whole-shelf
  instruments (`regression_audit`, and the round-trip check at the ID-set level)
  report nothing introduced anywhere.

- **A fixer for an empty `<dc:date>`** (`OPF-054`), the last of the four defects
  `epublift` handed over ([#5](https://github.com/veripublica/epubsana/issues/5)).
  An element with no content states no date, and `dc:date` is optional, so it is
  dropped.

  **What it does not do is the point of it.** epubveri's check is not "is the
  date empty" but "is it a valid W3C-DTF date", so the same id and the same
  message also cover `2022-09-08)` and `March 2019` — malformed values that still
  carry a date the author wrote. Those are **declined**: dropping one would
  destroy information the book has, and repairing one means guessing which
  characters are stray. The finding survives the repair, which is the honest
  outcome. An empty element whose `id` a `<meta refines>` targets is declined too.

  The claim to make for this fixer is therefore *"removes an empty `dc:date`,
  leaves a malformed one alone"* — not *"repairs `OPF-054`"*.

  Worth recording for anyone reading the id: `OPF-054` is **EPUB 2 only**. On
  EPUB 3 the identical condition is `OPF-053` at *Warning*, which never moves the
  validity line — the same version-scoped id split as the duplicate spine
  itemref, cutting the other way.

  **Zero occurrences on the 94-book shelf**, and zero on the earlier 171-book
  corpus: verified by injection end to end (an empty date takes a book from 1
  error to fully valid; a malformed one is left untouched with its finding
  intact), and its guards are covered by unit tests rather than by real books.
  The shelf's one nearby specimen is an `OPF-053` reading `2022-09-08)` — the
  declining case, on the id this fixer does not act on.

## [0.6.0] - 2026-08-05

Tracks `epubveri` from 0.5.15 to **0.9.7**, which corrects a false positive
epubsana had been repairing, and closes the only regression epubsana introduced
on the shared shelf.

### Changed

- **`epubveri` 0.9.7 is now the minimum, and the reason is correctness.**
  `opf.content_document.empty_title` was an EPUB 3 rule firing on EPUB 2 books
  (XHTML 1.1 types `<title>` as `<text/>`, which RELAX NG matches on empty; the
  non-empty assertion exists only in `epub-xhtml-30.sch`). While epubsana ran
  against the 0.5 line, `fix.empty_title` therefore filled `<title>` elements in
  books that were already valid. Measured on the 94-book shared shelf, epubsana's
  code unchanged and only the detector moving: **606 proposals → 3** (the three
  remaining are one EPUB 3 book, where the rule is right), and books receiving
  any proposal at all fall from 23 to 13. Upstream counts 8 books going INVALID →
  VALID for this cause alone.

  No epubsana code was wrong and no epubsana test could have caught it: a
  repairer inherits its detector's false positives whole. The bump itself needed
  no source change — the 0.8.0 `Options` API break does not reach this crate.

- **The bare-text fixer follows its finding to a new home.** epubveri removed
  `htm.epub2_dom.bare_text_in_body` in the 0.9 line because it duplicated the
  RELAX NG grammar; the same defect now arrives as
  `opf.content_document.schema_violation` with the message *stray text is not
  allowed directly in "body"*. `fix.bare_text_in_body` matches that shape
  (message prefix **and** `params[0] == "body"`, since one rule now spans a whole
  grammar) and is otherwise unchanged. Stray text in any other container —
  an `<ol>`, say — is **declined**: the correct wrapper there is an `<li>`, which
  asserts the text is a list item, and that is a judgement rather than a
  determinate repair. On the shelf the fixer clears **289 findings across 17
  documents**; without the re-target it would silently have done nothing.

### Added

- **Two fixers, from the families epubveri's 0.9 work newly exposed** (nineteen →
  twenty-one). Both were specified in `docs/FIXERS.md` before being coded, and
  both are one *message shape* wide rather than one rule wide — the unit that
  replaces "family" now that so much detection lives in the grammar.

  - `htm.obsolete_attribute`, the legacy `<a name>` anchor (`fix.anchor_name`,
    **AutoSafe**) — `name` on `<a>` predates `id` and XHTML 1.1 removed it. Where
    the element already carries an `id` with the **identical** value the anchor is
    declared the modern way too, so the `name` is a duplicate declaration and
    every `#fragment` targeting it resolves through the `id`. Dropping it loses
    nothing, which is what makes this the rare deletion that is AutoSafe.
    Declines an anchor with **no** `id` (renaming `name` → `id` would have to
    prove the value is an NCName *and* unique — it can manufacture a finding), one
    whose `id` differs (dropping breaks the fragment; an element cannot carry two
    ids), and every other attribute in the family, `<br clear>` included: its
    presentational intent has no single markup equivalent.

  - An empty `lang` / `xml:lang` (`fix.empty_lang`, **ConfirmNeeded**) — EPUB 2's
    grammar types them as a language tag and `""` is not one. The attribute is
    deleted. It is deliberately not AutoSafe: an element that declared
    "undetermined" will now inherit its parent's language, and a reading system
    acts on that (hyphenation, text-to-speech, CJK font selection). XHTML 1.1 has
    no valid spelling for "undetermined", so the choice is between an invalid
    document and inheritance — a decision about the book. A **malformed** tag
    (`lang="en_US"`) is declined: repairing it means guessing.

  On the 94-book shelf the two clear **exactly the 442 findings they target**
  (162 + 280) and no others, take one more book to fully valid, and introduce
  nothing. Both are single-book shapes on this shelf, which shows the repair is
  right for that shape rather than that the shape is common — the declines above
  are what makes that an acceptable basis to ship on.

- **`fixers::handled_rules()`** — every epubveri `rule` some fixer knows how to
  address. "Knows how to" is not "will": a listed rule may still be declined on
  any given book, and the distinction is the point. A rule *missing* from the
  list is a coverage gap; a listed rule that proposes nothing is a decision, and
  reading a plan's output cannot tell the two apart — a fixer that declines
  everywhere looks exactly like a fixer that does not exist. An embedder deciding
  whether to route a book through epubsana at all can now ask without running a
  plan.

### Fixed

- **`fix.manifest_dangling_item` no longer drops the navigation document.** A
  dangling manifest item carrying `properties="nav"` was dropped like any other,
  which cleared `RSC-001` and produced `opf.package.missing_nav_document` in its
  place — a book no more valid than before, and now without a table of contents.
  It declines instead, on the same principle as the existing spine guard: a
  repair that trades one error for another is not a repair. The `nav` token is
  matched, not the substring, and a declined item is also removed from the shared
  spine guard's arithmetic (that guard counts deletions that could happen, and
  this one no longer can).

  This was epubsana's own defect, live since 0.4.0 and present under both the old
  and the new detector. It was the **only** finding epubsana introduced anywhere
  on the 94-book shelf; with the guard in place the shelf run introduces nothing.

## [0.5.0] - 2026-07-18

Ten new fixers (nine → nineteen), tracking `epubveri` through 0.5.15, and the
structural fixers now read the EPUB 2 `&nbsp;` documents they previously couldn't.
Four whole error families are now handled end to end — entities, doctypes, NCX
internal consistency, and the EPUB 2 `<guide>`.

### Changed

- **Structural fixers now read EPUB 2 content documents that use DTD-only entities**
  (`&nbsp;` under an XHTML 1.1 DOCTYPE). roxmltree doesn't fetch the external DTD, so
  these documents didn't parse and every structural fixer silently declined them —
  the same reach gap epubveri closed in its issue #23. epubsana now declares the
  named entities the document uses in the DOCTYPE's internal subset in a working
  copy, parses that, and maps node byte ranges back to the original text it edits;
  the declarations never appear in the output, and the injection is bounded by the
  DOCTYPE (not a body `[1]`). Effect on the corpus: `fix.empty_title` proposals rise
  from 2153 to **2286** (+133) — the previously-unreadable documents that carry a
  title source now get one; the rest parse but are declined for having none. Also
  benefits `content_type_meta` and `bare_text_in_body`. Zero new regressions.

### Added

- **Two fixers completing the EPUB 2 `<guide>` family:**
  - `opf.guide.reference_missing_resource` (`RSC-007`, `fix.guide_dangling_reference`,
    ConfirmNeeded) — a `<guide>` reference whose `href` resolves to no resource in
    the container (on the corpus, a wrong extension like `rica.html` beside
    `rica.xhtml`) is dropped: it names a landmark no reader can reach and nothing
    records what file it meant. If dropping leaves the `<guide>` empty — invalid, and
    the element is optional — the `<guide>` is dropped too. Matches on the reported
    `href`; paths are not re-resolved. Dropping the reference also clears the
    co-emitted `OPF-031` ("not declared in the manifest").
  - `opf.guide.duplicate_reference` (`RSC-017`, `fix.guide_duplicate_reference`,
    ConfirmNeeded) — two or more references sharing the same `type` **and** `href`;
    the first is kept and the redundant repeats dropped. References with the same
    `type` but different `href` are not duplicates and are left alone.

  On the corpus this clears every occurrence in the affected books (missing-resource
  5 books, duplicate 1 book), and the `OPF-031` side effect too, with zero
  regressions. This closes the `opf.guide` family.

- **Two fixers completing the NCX internal-consistency family** (`RSC-005`):
  - `ncx.ids.duplicate_id` (`fix.ncx_duplicate_id`, ConfirmNeeded) — two or more NCX
    elements share an `id`. The first keeps it; each later duplicate is renamed to a
    unique value. NCX ids are not IDREF targets anywhere in an EPUB, so no reference
    is rewritten. Disjoint from the NCName fixer by construction (that one touches
    only ids occurring exactly once; a duplicate occurs more than once).
  - `ncx.play_order.duplicate` (`fix.ncx_play_order`, ConfirmNeeded) — navigation
    elements repeat a `playOrder`. Every `playOrder` is renumbered to its 1-based
    document-order position (the canonical assignment), making the values unique.
    `playOrder` is only a hint; the reading order a system follows is the spine,
    untouched.

  With the existing NCName and `dtb:uid` fixers, this **handles the NCX
  internal-consistency family end to end**: the one remaining member,
  `ncx.page_target.invalid_type`, is deliberately **declined** — a bad `@type`
  (`front`/`normal`/`special`) has no determinate replacement, and guessing the page
  category would be inventing. On the corpus both new fixers clear every occurrence
  in the two affected books (`duplicate_id` 3→0, `play_order` 3→0 each), zero
  regressions.

- **Two fixers for an obsolete or unrecognized DOCTYPE** (`HTM-004`), closing out
  the `htm.doctype` family:
  - `htm.doctype.epub3_obsolete_public_id` (`fix.doctype_html5`, AutoSafe) — an
    EPUB 3 document's DOCTYPE carrying a PUBLIC identifier is reduced to HTML5's
    only legal form, `<!DOCTYPE html>`. Declines a DOCTYPE with an internal subset,
    whose `[ … ]` declarations HTML5 can't carry.
  - `htm.doctype.epub2_unrecognized_public_id` (`fix.doctype_xhtml11`,
    ConfirmNeeded) — an EPUB 2 DOCTYPE whose identifier is a **malformed XHTML 1.1**
    id (names 1.1 / the `xhtml11.dtd` but mistypes the exact string) is canonicalized
    to the recognized form. **Declines a document declaring a genuinely different DTD**
    (XHTML 1.0, a bare `<!DOCTYPE html>`, OEB): relabeling it to 1.1 would assert a
    content model epubsana can't verify and risks trading the finding for
    content-model errors. On the corpus the one affected book (XHTML 1.0 Strict, 77×)
    is declined, correctly.

  Both are surgical on the DOCTYPE only and bound it by its own closing `>` (never a
  body `[1]`), the lesson of the upstream bracket bug. The `htm.doctype` family is
  now handled end to end — every finding gets a repair or a principled decline —
  though not "every occurrence rewritten"; the decline is the "never guess" rule at
  work. See `docs/COVERAGE.md`.

- **A fixer for an entity reference missing its closing `;`**
  (`RSC-016` / `htm.entity.missing_semicolon`, *fatal*) — `&nbsp` for `&nbsp;`.
  A recognized name is replaced by the character it denotes (well-formed with or
  without a DTD); one of the XML-predefined five (`&amp` …), whose character is
  the bare delimiter itself, is closed with the missing `;` instead. The match is
  boundary-checked, so a correct `&nbsp;` and a longer entity that merely starts
  with the name are never touched; an unrecognized name is declined.

  This **completes the `htm.entity` family** — the first error family epubsana
  covers end to end: every entity defect epubveri reports (undeclared, and now
  unterminated) has a repair. See `docs/COVERAGE.md` for the family map this
  closure is measured against.

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

- **Track `epubveri` 0.5.15** (from 0.5.9). No source change across the whole span
  — the `rule`/`params` contract held every bump; the effects are behavioural.
  - [epubveri#23](https://github.com/veripublica/epubveri/issues/23) (0.5.12): EPUB 2
    documents with DTD-declared entities (`&nbsp;` under an XHTML 1.1 DOCTYPE) now
    parse, so a class of false `RSC-012` "fragment not defined" findings is gone.
    `RSC-012` drops from 1247 to 172 — the 172 are the genuinely dangling fragments,
    the ~1075 removed were the detector failing to read a valid document and calling
    its ids absent. `empty_title` findings rise +157 as the same documents become
    readable.
  - [epubveri#25](https://github.com/veripublica/epubveri/issues/25) (0.5.12): a
    regression in 0.5.10/0.5.11 that turned any EPUB 2 document with a `[` in its
    body (a footnote marker) into a false fatal — 78 across 11 corpus books on
    0.5.11, zero on 0.5.12.
  - **Content-model validation** (0.5.13 EPUB 2, 0.5.15 EPUB 3): EPUB 2 books are
    now checked against XHTML 1.1 + OPS 2.0.1 instead of HTML5, and EPUB 3 nesting /
    IDREF rules are enforced. This is verdict-changing: `opf.content_document.schema_violation`
    rises from 16 to **32 books**, and new `RSC-005` sub-codes appear
    (`htm.epub2_dom.nested_anchor`, `htm.epub2_dom.html5_only_element`). epubsana
    does not yet repair these classes.

  Net on the 171-book corpus: **25** books brought all the way to valid (was 30 on
  0.5.12), still **zero regressions**. The drop is not lost coverage — every fixer
  fires identically; it is the content model finding real defects epubsana does not
  yet fix. Measured: **14 books are now blocked from fully-valid *only* by a new
  content-model finding**, which is the ROI case for a content-model fixer.

  The manifest floor is now `epubveri = "0.5.15"` — earlier versions either misreport
  (`RSC-012` on 0.5.9, the #25 fatal on 0.5.10/0.5.11) or predate content-model parity.

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
