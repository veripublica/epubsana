# epubsana — Fix Catalogue

**How each finding is repaired, and why it's safe.** This is the specification a
reviewer reads *before* the code: for every epubveri finding epubsana handles,
it states exactly what is changed, why that change is content-preserving, and
the conditions under which epubsana **declines** and leaves the finding alone.

epubsana never guesses. If a finding has no determinate, safe fix, it is
reported and left untouched — so this catalogue is deliberately narrow, and
grows one carefully-argued entry at a time.

- This is the contributor/reviewer companion to the user-facing
  [USAGE.md](./USAGE.md).
- Each entry names the fixer's `fix_id`; find it in `src/fixers.rs` to check the
  code against the spec.
- **Tiers:** `AutoSafe` = exactly one correct, content-preserving fix, safe to
  apply unattended. `ConfirmNeeded` = a good fix that makes a visible change;
  the caller should approve it. (See [USAGE.md](./USAGE.md#the-interactive-workflow).)

---

## Summary

| epubcheck ID | rule sub-code | Tier | Issue | Fix |
| --- | --- | --- | --- | --- |
| `RSC-016` | `htm.entity.undeclared` | AutoSafe | XHTML uses HTML named entities with no DTD to declare them | [Replace each with the character it denotes](#rsc-016--undeclared-html-entities) |
| `RSC-016` | `htm.entity.missing_semicolon` | AutoSafe | A named entity reference lacks its closing `;` (`&nbsp`) | [Replace with the character, or close the reference](#rsc-016--entity-reference-missing-its-semicolon) |
| `RSC-005` | `ncx.ids.invalid_ncname` | ConfirmNeeded | An NCX `id` isn't a valid XML NCName | [Sanitize it to a valid, unique NCName](#rsc-005--invalid-ncx-id-ncname) |
| `RSC-005` | `opf.content_document.invalid_content_type_meta` | ConfirmNeeded | A legacy `<meta http-equiv="Content-Type">` has the wrong value | [Normalize to a single HTML5 `<meta charset="utf-8">`](#rsc-005--content-type-encoding-declaration) |
| `NCX-001` | *(none)* | ConfirmNeeded | The NCX `dtb:uid` disagrees with the package identifier | [Set `dtb:uid` to the package's unique identifier](#ncx-001--ncx-dtbuid-mismatch) |
| `RSC-005` | `opf.content_document.empty_title` | ConfirmNeeded | An XHTML `<title>` element is empty | [Fill it from the book's own TOC label, else its first heading](#rsc-005--empty-title) |
| `RSC-020` | `opf.manifest_item.unencoded_space_in_href` | AutoSafe | A manifest `href` contains a raw space | [Percent-encode the space as `%20`](#rsc-020--unencoded-space-in-a-manifest-href) |
| `OPF-014` | `opf.content_document.property_used_undeclared` | AutoSafe | A content document uses a feature its manifest item doesn't declare | [Add the token to that item's `properties`](#opf-014--undeclared-content-property) |
| `PKG-006` | *(none)* | AutoSafe | The `mimetype` entry is not first in the ZIP, as OCF requires | [Re-emit it first and stored, touching no content](#pkg-006--mimetype-is-not-the-first-entry) |
| `RSC-005` | `opf.content_document.schema_violation` (stray text / inline element / incomplete content, in `<body>` or `<blockquote>`) | ConfirmNeeded | EPUB 2 text or inline elements sit where the grammar requires block content | [Wrap each run in one `<div>`, leaving whitespace alone](#rsc-005--non-block-content-in-body-or-blockquote-epub-2) |
| `RSC-001` | `opf.manifest_item.missing_resource` | ConfirmNeeded | A manifest `<item>` declares a resource the container doesn't hold | [Drop the item, and every reference that named it](#rsc-001--dangling-manifest-item) |
| `OPF-049` | `opf.spine.itemref_idref_not_in_manifest` | ConfirmNeeded | A spine `<itemref>` names a manifest id that doesn't exist | [Drop the itemref](#opf-049--dangling-spine-itemref) |
| `OPF-034` / `RSC-005` | `opf.spine.duplicate_itemref` | ConfirmNeeded | The spine lists the same manifest item more than once | [Keep the first occurrence, drop the later ones](#opf-034--rsc-005--duplicate-spine-itemref) |
| `HTM-004` | `htm.doctype.epub3_obsolete_public_id` | AutoSafe | An EPUB 3 document's DOCTYPE carries an obsolete PUBLIC identifier | [Reduce it to `<!DOCTYPE html>`](#htm-004--obsolete-or-unrecognized-doctype) |
| `HTM-004` | `htm.doctype.epub2_unrecognized_public_id` | ConfirmNeeded | An EPUB 2 document's DOCTYPE isn't a recognized XHTML 1.1 / OEB identifier | [Canonicalize a malformed XHTML 1.1 id; decline a genuinely different DTD](#htm-004--obsolete-or-unrecognized-doctype) |
| `RSC-005` | `ncx.ids.duplicate_id` | ConfirmNeeded | Two or more NCX elements share an `id` | [Keep the first, rename later duplicates uniquely](#rsc-005--ncx-internal-consistency) |
| `RSC-005` | `ncx.play_order.duplicate`, `…target_mismatch`, `…gap` | ConfirmNeeded | The NCX's `playOrder` values are inconsistent | [Reassign densely by document order, same target → same number](#rsc-005--ncx-internal-consistency) |
| `RSC-007` | `opf.guide.reference_missing_resource` | ConfirmNeeded | A `<guide>` reference points at a resource that doesn't exist | [Drop the reference; drop the guide if it empties](#rsc-007--rsc-017--rsc-012--guide-references) |
| `RSC-017` | `opf.guide.duplicate_reference` | ConfirmNeeded | Two `<guide>` references share a `type` and `href` | [Keep the first, drop the duplicates](#rsc-007--rsc-017--rsc-012--guide-references) |
| `RSC-012` | `opf.guide.reference_fragment_not_defined` | ConfirmNeeded | A `<guide>` reference's `#fragment` resolves to no `id` in a target that does exist | [Drop the fragment, keep the document](#rsc-007--rsc-017--rsc-012--guide-references) |
| `RSC-005` | `htm.obsolete_attribute` (`params[0] == "name"`) | AutoSafe | A legacy `<a name>` anchor duplicating the element's own `id` | [Drop the `name` attribute](#rsc-005--a-legacy-name-attribute-on-a) |
| `RSC-005` | `opf.content_document.schema_violation` (empty `lang`/`xml:lang`) | ConfirmNeeded | An empty language tag, which EPUB 2's grammar does not allow | [Delete the attribute](#rsc-005--an-empty-lang--xmllang) |
| `RSC-005` | `opf.content_document.schema_violation` (`params[0] == "id"`) | ConfirmNeeded | An `id` that is not a valid XML NCName (on the shelf: it starts with a digit) | [Rename it, moving every reference with it](#rsc-005--an-id-that-is-not-a-valid-ncname-the-first-cross-file-fixer) |
| `RSC-005` | `opf.package.schema_violation` | AutoSafe | An EPUB 3 attribute on an EPUB 2 package document | [Delete it, once verified it says nothing the book does not](#rsc-005--an-epub-3-attribute-on-an-epub-2-package-document) |
| `RSC-005` | `opf.content_document.duplicate_id` | ConfirmNeeded | Two or more elements in one document share an `id` | [Keep the first, rename the later ones](#rsc-005--a-duplicate-id-in-a-content-document) |
| `RSC-007` | `opf.content_document.reference_missing_resource` | ConfirmNeeded | A link's path is stale, but the file it names is still in the book | [Repoint the path, carry the fragment across](#rsc-007--a-reference-whose-path-is-wrong-but-whose-target-is-in-the-book) |
| `OPF-030` / `RSC-005` | `opf.package.unique_identifier_unresolved`, `opf.package.opf_identifier_not_empty` | ConfirmNeeded | The package's declared unique identifier resolves to nothing usable | [Attach the declared id to the book's one real identifier](#opf-030--rsc-005--the-packages-declared-identifier-points-at-nothing-usable) |
| `RSC-005` | `htm.epub2_dom.nested_anchor` | ConfirmNeeded | An `<a>` with only an `id` wraps a real link | [Unwrap it, moving the `id` to the child](#rsc-005--an-anchor-target-wrapped-around-a-link) |
| `OPF-054` | *(none)* | ConfirmNeeded | A `<dc:date>` holds no date at all (EPUB 2) | [Drop the empty element; never touch a non-empty one](#opf-054--an-empty-dcdate-epub-2) |

**A note on structural fixers.** Fixers that must locate an element (rather than
match a token) parse the document with `roxmltree` using `allow_dtd: true`, the
same option epubveri uses. NCX files and many XHTML documents declare a
`DOCTYPE`, which roxmltree's default parser rejects; matching epubveri's setting
means a structural fixer sees exactly the documents epubveri did.

For **content documents**, matching epubveri means one more step: an EPUB 2 XHTML
document may use `&nbsp;` and friends under an XHTML 1.1 DOCTYPE that declares them
only in its *external* DTD, which roxmltree does not fetch — so the document won't
parse and, before this, every structural fixer silently declined it. epubsana now
does what epubveri does (its issue #23): it declares those named entities in the
DOCTYPE's internal subset **in a working copy**, parses that, and maps each node's
byte range back to the original text it edits. The declarations exist only to
locate nodes; they never appear in the output, and the injection is bounded by the
DOCTYPE itself (not a `[1]` in the body — the lesson of epubveri's bracket bug). If
a document *still* won't parse, the fixer declines. Measured effect: **133 more
`empty_title` findings across the corpus now get a proposal** (the rest of the
previously-unreadable ones parse but are declined for having no in-book title
source — the ordinary "never invent" rule).

---

## RSC-016 — undeclared HTML entities

**Finding.** `htm.entity.undeclared`, at **fatal** severity. An XHTML document
references an HTML named entity — `&nbsp;`, `&mdash;`, `&eacute;`, … — without a
DTD that declares it. epubveri reports the entity's name (in `params[0]`) and the
file. Grouped per file (a single book can have thousands).

It is fatal, not merely an error, because an undeclared entity makes the document
**not well-formed XML**: a reading system does not render it at all. This is the
single most common reason a real book fails to open — and it is why clearing it
is exactly what `--goal openable` is for.

**Fix** (`fix.html_entities`, AutoSafe). For each recognized entity, replace
every `&name;` occurrence in the file with the exact Unicode character that
entity denotes (from a curated Latin-1 + General-Punctuation + common-symbol
table). Example: `&nbsp;` → U+00A0, `&mdash;` → `—`.

**Why it's safe.** These are standard HTML named entities; substituting the
character they denote changes only the *encoding* of that character, not the
rendered content. The result no longer relies on an undeclared entity, so the
document becomes well-formed and the finding clears.

**When it declines.** Any entity **not** in the curated table is left untouched
(and stays reported). The table is deliberately conservative — an unknown or
ambiguous entity is never guessed. The XML-predefined five (`&amp;` `&lt;`
`&gt;` `&quot;` `&apos;`) are always declared and so never appear here.

---

## RSC-016 — entity reference missing its semicolon

**Finding.** `htm.entity.missing_semicolon`, at **fatal** severity. A named entity
reference lacks its closing `;` — `&nbsp` where `&nbsp;` was meant. A `&` not
closed by `;` is not well-formed XML, so the document does not parse and does not
open — the same Fatal, and the same `--goal openable` stakes, as the
undeclared-entity case above. epubveri's scanner reports the recognized entity
**name** in `params[0]`. This is the sibling that completes the `htm.entity`
family: with `htm.entity.undeclared`, every entity defect epubveri reports has a
repair.

**Fix** (`fix.entity_missing_semicolon`, AutoSafe). Per file, for each recognized
name, replace the unterminated `&name` with:

- **the character it denotes**, when the name is one we map (reusing the
  `html_entities` table) — this both closes and resolves the reference, leaving
  text that is well-formed with or without a DTD (`&nbsp` → U+00A0);
- **`&name;`** (the semicolon inserted), when the name is one of the XML-predefined
  five (`amp`/`lt`/`gt`/`quot`/`apos`), whose denoted character is itself
  `&`/`<`/`>`/`"`/`'`. Substituting the character there would put the bare
  delimiter straight back, so the repair is to *close* the reference, not resolve
  it (`&amp` → `&amp;`).

The match is boundary-checked: `&name` is repaired **only** where the next
character is neither `;` (already terminated — nothing to do) nor a name character
(`&notin;` is not an unterminated `&not`). So a correct `&name;` elsewhere in the
file, and a longer entity that merely starts with this name, are never touched.

**Why it's safe.** For a mapped entity, the character is exactly what the reference
denotes — the same content-preserving substitution as the undeclared case, and it
removes the malformed reference outright. For a predefined entity, inserting the
one missing `;` is the single change that makes the reference well-formed and
denotes nothing new. In both cases the document becomes parseable and the fatal
clears.

**When it declines.** An unrecognized name — not in the map and not one of the
predefined five — is left untouched and stays reported. As everywhere, an unknown
entity is never guessed.

---

## RSC-005 — invalid NCX id (NCName)

**Finding.** `ncx.ids.invalid_ncname`. An `id` attribute anywhere in the NCX is
not a valid XML NCName — e.g. a raw UUID that starts with a digit
(`51100e1e-…`), or a brace-wrapped GUID (`{0F5794B8-…}`). An NCName may not start
with a digit, nor contain characters such as `{`, `}` or `:`. epubveri reports
the offending value in `params[0]`, with the file and position.

**Fix** (`fix.ncx_ncnames`, ConfirmNeeded). Derive a valid NCName from the value,
preserving as much of it as possible:

1. Drop characters not allowed in an NCName (`{`, `}`, `:`, whitespace, …).
2. If the result doesn't start with a letter or `_`, prefix `id_`.
3. If that collides with another id in the NCX, suffix `-2`, `-3`, … until
   unique.

Then rewrite **only** that one `id` attribute in place. Examples:
`51100e1e-…` → `id_51100e1e-…`; `{0F5794B8-…}` → `id_0F5794B8-…`.

**Why it's safe.** NCX ids are not IDREF targets anywhere in an EPUB, so making
one valid needs no reference rewriting — nothing points at it. The uniqueness
suffix guarantees the change can't introduce a duplicate-id error, and the
transformation is otherwise content-preserving.

**When it declines.** If the `id="…"` attribute doesn't occur textually exactly
once (so the surgical rewrite would be ambiguous — e.g. a duplicated bad id), or
if nothing valid remains after sanitizing, the fixer leaves it untouched.

---

## RSC-005 — content-type encoding declaration

**Finding.** `opf.content_document.invalid_content_type_meta`. A content
document carries a legacy `<meta http-equiv="Content-Type" content="…">` whose
value isn't exactly `text/html; charset=utf-8` — real cases include a bogus MIME
(`http://www.w3.org/1999/xhtml; charset=utf-8`) or a missing space
(`text/html;charset=utf-8`). This finding carries no `params`, so the element is
located by parsing the document.

**Fix** (`fix.content_type_meta`, ConfirmNeeded). Normalize the document's
encoding declaration to the EPUB 3.3 / HTML5 form. Per file:

- If a valid `<meta charset="…">` already exists, keep it and remove every
  legacy `<meta http-equiv="Content-Type">`.
- Otherwise rewrite the first such meta to `<meta charset="utf-8"/>` and remove
  any remaining encoding metas.

The result is exactly **one** `<meta charset="utf-8"/>`. Each affected meta's
exact byte range is found by parsing, and edited surgically.

**Why it's safe.** EPUB content documents must be UTF-8, so declaring
`charset="utf-8"` states the required, already-true encoding — it does not
re-encode anything. Collapsing to a single declaration also prevents a
`conflicting_encoding_declarations` error from newly appearing. Producing the
HTML5 form (not the legacy `http-equiv` one) follows the
[EPUB 3.3 reference standard](./USAGE.md#reference-standard).

**When it declines.** If the document doesn't parse, or if any encoding meta
declares a **non-UTF-8** charset (epubsana will not blindly re-encode content),
the file is left untouched.

---

## NCX-001 — NCX dtb:uid mismatch

**Finding.** `NCX-001`. The NCX's `<meta name="dtb:uid" content="…">` doesn't
match the package's unique identifier. This finding carries no `rule`/`params`,
but its epubcheck ID is unambiguous, so epubsana dispatches on the ID.

**Fix** (`fix.ncx_dtb_uid`, ConfirmNeeded). Set the NCX `dtb:uid` content to the
package's unique identifier — the `dc:identifier` the OPF `unique-identifier`
attribute points at. The value is resolved structurally
(`META-INF/container.xml` → OPF → `unique-identifier` → matching
`dc:identifier`), mirroring epubveri's own resolution exactly, and **only** the
`content` value of that one meta is rewritten (every other attribute and the
element's formatting are preserved).

**Why it's safe.** Making `dtb:uid` equal the package identifier is precisely
what the check requires; the `dtb:uid` value is not referenced elsewhere, so
nothing else is affected.

**When it declines.** If the package identifier can't be resolved (a broken or
missing OPF `unique-identifier` / `dc:identifier`), or the NCX won't parse, the
fixer leaves it untouched rather than invent a value.

---

## RSC-005 — empty `<title>`

**Finding.** `opf.content_document.empty_title`. An XHTML content document has a
`<title></title>` with no text. HTML requires a non-empty title, and this is the
**most widespread defect in the real-world corpus** — whole libraries ship
generated documents whose title element is empty.

**Fix** (`fix.empty_title`, ConfirmNeeded). Fill the title with text **taken from
the book itself**, in this order:

1. the **label the book's own table of contents gives this document** — its NCX
   `navLabel/text`, or the EPUB 3 nav document's `<a>` text, for the entry whose
   target resolves to this document (the fragment is ignored: an entry pointing
   *into* a document still names it);
2. failing that, the **document's own first heading** (`h1`–`h6`), whitespace
   collapsed to one line.

The text is XML-escaped and only the empty `<title>` element is rewritten.

**Why it's safe.** The title is never *invented*: both sources are the book's own
words for that document, authored by whoever made the book. The title element is
document metadata — it is not part of the rendered text — so filling it changes
nothing a reader sees in the content, and it clears a genuine content-model
violation. It is `ConfirmNeeded` rather than `AutoSafe` precisely because it adds
visible metadata: the user sees the exact text before approving it.

**When it declines.** When the book names the document **nowhere** — no TOC entry
and no heading (measured: ~7% of the corpus's empty titles, typically image-only
cover and divider pages) — the fixer leaves it alone and the finding stays
reported. epubsana deliberately does **not** fall back to the book's `dc:title`:
stamping the book's name onto every chapter is a guess about intent, not a
repair. It also declines a document that won't parse, or whose title turns out
not to be empty after all (a stale finding never overwrites real text).

---

## RSC-020 — unencoded space in a manifest `href`

**Finding.** `opf.manifest_item.unencoded_space_in_href`. A manifest `<item>`'s
`href` contains a raw space; epubveri reports the offending href in `params[0]`.

**Fix** (`fix.manifest_href_spaces`, AutoSafe). In that one manifest item,
percent-encode each space in the `href` as `%20`. The quote style and every other
attribute of the element are preserved.

**Why it's safe.** An `href` is a URL, and a space is not a legal URL character —
`%20` is its one correct spelling. The **file is not renamed**: a space in a ZIP
entry name is perfectly valid, and `%20` resolves back to exactly the same entry,
so every reference still points where it did. Only the spaces epubveri flagged
are encoded; nothing else in the href is touched.

**When it declines.** If the OPF won't parse, or no manifest item carries the
reported href verbatim, no edit is made.

---

## OPF-014 — undeclared content property

**Finding.** `opf.content_document.property_used_undeclared`. A content document
uses a feature — `scripted`, `svg`, `remote-resources`, or `switch` — that its
manifest `<item>` does not declare. epubveri names the property in `params[0]`.

**Fix** (`fix.content_properties`, AutoSafe). Add the token to the `properties`
attribute of the manifest item whose `href` resolves to that document (existing
tokens are kept; the attribute is created if absent). The item's href is resolved
the way a reading system resolves it — relative to the OPF's directory,
percent-decoded, with `.`/`..` normalized.

**Why it's safe.** epubveri has already *proven* the usage by finding it in the
document, so the declaration is not a guess — it is the manifest being made to
tell the truth about a document that is not itself modified. EPUB 3.3 requires
exactly this declaration.

**When it declines.** If the OPF won't parse, no manifest item resolves to the
document, or the property is already declared, nothing is changed.

---

## PKG-006 — `mimetype` is not the first entry

**Finding.** `PKG-006` (no `rule` sub-code — the code is unambiguous on its own,
and its subject is the container itself, so there is nothing to disambiguate).
The archive has a `mimetype` entry, but it is not the first one. OCF requires the
`mimetype` entry to come first and to be stored uncompressed, so that a reader
can identify the file by reading its opening bytes.

**Fix** (`fix.mimetype_packaging`, AutoSafe). Re-emit the `mimetype` entry first
and stored. Every other entry keeps its original order, bytes and compression.

**Why it's safe.** This is the rare fix that changes **no content whatsoever** —
not one byte of any entry, `mimetype` included. Only the entry's *position* in
the archive and its compression method change, which is exactly what the finding
is about, and OCF permits exactly one correct answer: first, and stored. Nothing
inside the book can be corrupted by it because nothing inside the book is read or
rewritten.

**When it declines.** If the archive has no `mimetype` entry at all, there is
nothing to move — epubsana will not create one, because inventing a mimetype is
asserting what the file *is* rather than repairing how it is packaged.

**Note — this fix used to happen invisibly.** Through 0.3.2 the writer always
re-emitted `mimetype` first and stored, so merely producing output repaired this
defect with no proposal and no approval. That contradicted epubsana's first
guarantee, so the writer now preserves packaging exactly and this fixer proposes
the repair in the open, where you can see it and decline it.

---

## RSC-005 — non-block content in `<body>` or `<blockquote>` (EPUB 2)

**Finding.** `opf.content_document.schema_violation`, message *stray text is not
allowed directly in "body"; wrap it in an element*, with `params[0] == "body"`.
An EPUB 2 content document has text sitting directly inside `<body>`, with no
block-level element around it. XHTML 1.1 requires `<body>` to contain block-level
content, so this is invalid there. (EPUB 3 is HTML5, where `<body>` accepts flow
content directly — so the finding only ever arises on EPUB 2, by the grammar
rather than by a version test.)

**This finding used to have a rule of its own,** `htm.epub2_dom.bare_text_in_body`.
epubveri removed that check in the 0.9 line because it duplicated what the RELAX NG
grammar already reported, and the detection moved into `schema_violation` — same
book, same file, same count, verified file-by-file upstream. Since `schema_violation`
is one rule spanning a whole grammar, matching it takes two conditions:

- **`params[0]` is the containing element**, and epubsana acts on the containers
  whose XHTML 1.1 content model **requires block content and admits `<div>`**:
  `body` and `blockquote`. Stray text in an `<ol>` is a real finding too, but its
  correct wrapper is an `<li>` — which asserts the text *is* a list item, a
  judgement rather than a determinate repair. `<head>` wants a `<title>`, which is
  a different repair again. Every other container is declined.

  Measured on the then-125-book shelf (2026-08-09), that rule is not a compromise — it is the whole
  population: **stray text is reported in exactly two containers**, `blockquote`
  (2,508 findings) and `body` (289), and nothing else. `ol`, `ul` and `head`
  appear only under *incomplete content*, at one or two findings each.
- **The message prefix identifies the kind of violation.** `params[0]` cannot do
  it alone (`element "body" is not allowed here` carries the same param), and the
  prefix cannot do it alone either (it matches the `<ol>` case). This is a
  coupling to English message text, and it is the only discriminator the finding
  offers; it fails in the safe direction, because a reworded message makes the
  fixer go quiet rather than edit the wrong node.

epubsana still locates the text itself by parsing the document, as it did when
the rule was its own — so it never depended on how precisely the finding is
anchored. (epubveri restored per-run positions and `…/text()[n]` paths in 0.9.7,
its issue #68, after they were lost in the move; useful to a repairer that
addresses by position, which this one deliberately is not.)

**It is not only text, and treating it as only text was wrong.** XHTML 1.1 wants
*block-level* content in `<body>`, and a converter leaves inline **elements**
there just as often as it leaves text: on the 94-book shelf, `<body>` holds 281
stray `<a>`, 92 `<br>`, 5 `<img>`, 4 `<span>` and a few `<sup>`/`<sub>`, reported
as `element "a" is not allowed here; expected one of … "div" …`. The detector's
own `params` name `div` among the elements allowed at that position, so the
wrapper this fixer already uses is the one the grammar asks for.

Crucially the two interleave. Measured on the shelf, of 244 runs of non-block
content in `<body>`, **116 mix text and inline elements** and only 128 are text
or elements alone. A fixer that wrapped text and left the `<a>` beside it outside
would split one rendered line into two blocks — and two fixers each owning half a
run would collide, with the second one silently finding nothing to do while the
report claimed it had applied. So **one fixer owns the whole region.**

**The same defect is reported twice, and one wrap clears both.** A
`<blockquote>` holding only text also *has incomplete content*, because its model
requires at least one block child — so the shelf carries `stray text is not
allowed directly in "blockquote"` (2,508) and `element "blockquote" has
incomplete content` (3,009) over the same elements. Verified on real books rather
than reasoned: in one file `{incomplete 53, stray 54, span 1}` becomes `{}`, and
in another `{incomplete 96, span 98, img 7}` becomes `{img 7}` — the inline
elements inside the blockquote clear as a side effect, since they are part of the
run being wrapped. The fixer therefore also triggers on the incomplete-content
message for those two containers.

**Fix** (`fix.bare_text_in_body`, ConfirmNeeded). Wrap each **maximal run** of
non-block content — stray text and inline elements together, in document order —
in a single `<div>`, inside every `<body>` and every `<blockquote>`, grouped one
proposal per document. The two never overlap: a `<blockquote>` is a block element
and therefore ends a run in its parent, while its own children are walked
separately. The wrapper goes around
the run's **non-whitespace span only**: `"\n\n\n50\n"` becomes
`"\n\n\n<div>50</div>\n"`, so the document's existing line breaks and
indentation are untouched, and `"text <a>link</a>"` becomes
`"<div>text <a>link</a></div>"` — one block, as it rendered before.

The `fix_id` stays `fix.bare_text_in_body` even though the name is now narrower
than the behaviour: it is a published identifier that `epublift` reads out of our
JSON, and renaming it would break a consumer to make a docs sentence tidier.

**Why it's safe.** The text itself is never altered — not a character is added,
removed or re-ordered; a wrapper appears around it and nothing else in the
document is touched. `<div>` is chosen deliberately over `<p>`:

- It makes **no claim about what the text is.** In the real corpus this text is
  usually a chapter title or a stray paragraph a converter left behind; calling
  it a paragraph would be a guess about intent, and calling it a heading more so.
- It **renders where it already rendered.** A reading system already lays bare
  text out in an anonymous block, which is exactly what a `<div>` is; a `<p>`
  would add default margins and push the page around.

**Which elements count as inline** is XHTML 1.1's own Inline set — `a`, `abbr`,
`acronym`, `b`, `bdo`, `big`, `br`, `cite`, `code`, `dfn`, `em`, `i`, `img`,
`input`, `kbd`, `label`, `map`, `object`, `q`, `samp`, `select`, `small`, `span`,
`strong`, `sub`, `sup`, `textarea`, `tt`, `var`, `button`. The list is the
reference standard's, not a guess, and everything outside it ends the run.

**When it declines — and the biggest decline is the important one.**

- **An element XHTML 1.1 does not have at all.** `figure` (151 findings on the
  shelf), `section` (92), `figcaption` (90), `center`, and a stray `li` are all
  reported at the same position by the same message, and wrapping one in a `<div>`
  would **not** clear its finding: the element itself is unknown to the grammar,
  so the violation moves rather than goes away. Repairing those means *renaming*
  them, which is a different operation and a different argument. They end the run
  and are left alone — so this fixer covers roughly 387 of the 726 body-level
  findings, and says so rather than claiming the family.
- **`div` is not among the expected elements** the finding lists. Then the
  grammar is objecting to something other than block-level placement, and the
  wrapper would not be the repair.
- **Any container outside `{body, blockquote}`.** An inline element misplaced
  inside an `<ol>` is a real finding with a different correct answer, exactly as
  for stray text.
- **Incomplete content a wrapper cannot complete.** `<body>` carries 403
  incomplete-content findings on the shelf and this fixer clears **2** of them —
  the rest are bodies whose only children are `<figure>`/`<section>`, elements
  XHTML 1.1 does not have at all. Wrapping one changes nothing; the repair there
  would be renaming, which epubsana deliberately does not do (see
  `docs/COVERAGE.md`, 2026-08-06). Expect the same shortfall on `blockquote`:
  the ceiling is the part whose children are text or inline, not the whole 3,009.
- If the document doesn't parse, or it has no `<body>`, nothing is changed.

**Whitespace is never wrapped.** Text nodes that are only whitespace — the line
breaks between sibling elements — are left exactly as they are. They are not the
defect (epubveri does not report them, and XHTML does not object to them), and
they outnumber the real ones by more than a hundred to one: across the corpus's
six affected books, `<body>` holds **7,594** whitespace-only text nodes against
**54** real ones. A fixer that wrapped them all would bloat every book with
thousands of empty `<div>`s.

---

## RSC-001 — dangling manifest item

**Finding.** `opf.manifest_item.missing_resource`. A manifest `<item>` declares a
resource that isn't in the container: `<item id="cover-1" href="cover-1.jpg"/>`
with no such entry. epubveri reports the item's `id` in `params[0]` and the
unresolvable `href` in `params[1]`.

**Fix** (`fix.manifest_dangling_item`, ConfirmNeeded). Drop the `<item>` element —
**and, in the same proposal, every reference in the package that named it**:

1. any `<spine><itemref idref="…"/>` whose `idref` is the dropped item's `id`;
2. the legacy `<meta name="cover" content="…"/>` if its `content` is that `id`.

These are not separate fixes and are deliberately not offered as separate
choices. A user who approved the item drop but declined the spine drop would be
left with an `OPF-049` that epubsana itself created — a book worse than the one
it started with. One decision, one proposal, one atomic edit.

**Why it's safe.** A manifest item is a claim that a resource is part of the
publication. When the bytes aren't there, the claim is simply false, and no
amount of judgement recovers them — the entry cannot be repaired *into* anything,
because nothing in the book records what it was meant to point at. So the only
options are "drop it" or "keep the error"; there is no third option a human would
pick, which is what makes the fix determinate.

Nothing readable is lost by the cascade either. A spine entry naming an item
whose file is missing is a position in the reading order that no reading system
can render; dropping it removes a hole, not a chapter. The cover `<meta>` is the
same argument one level up: it points at a pointer to a hole, and the book had no
cover before the fix or after it.

It is `ConfirmNeeded` rather than `AutoSafe` because it is a **deletion** that can
shorten the reading order and can remove the book's cover declaration. Both are
visible in a reading system's UI, and epubsana does not delete visible structure
unattended, however sound the argument.

**We do not re-resolve the href — and that is the point.** epubveri hands us the
`id` in `params[0]`; the fixer finds the element by that id and never touches
path resolution. So the "is this href a remote URL rather than a container path?"
question does not arise here: whether a remote `href` is a missing resource is
epubveri's call, and if it ever answers that wrongly, the fix is an epubveri
issue, not a guard bolted on here. epubveri detects; epubsana repairs what it
reports. A second opinion about what counts as missing would make epubsana a
second detector.

(For the record, epubveri already gets this right: its `RSC-001` site is guarded
by `if !is_external(href)`, so a remote `href` never reaches us as a missing
resource. That is a reason to trust the boundary, not a reason to duplicate the
check — if it ever regressed, a guard here would hide the bug rather than fix it.)

**When it declines.**

- If the OPF won't parse, or no manifest item carries the reported `id`.
- **If the cascade would empty the `<spine>`.** A book whose every spine entry
  names a missing resource has no reading order at all, and emitting a spine-less
  EPUB trades this finding for a different broken book rather than repairing
  anything. epubsana reports it and leaves it for a human.
- **If the item is the navigation document** (`properties="nav"`). A publication
  that declares a nav document must have one, so dropping the item clears
  `RSC-001` and produces `opf.package.missing_nav_document` in its place: the
  book is no more valid than before, and now has no table of contents either.
  This is the spine guard's principle one level down — a repair that trades one
  error for another is not a repair. The `nav` **token** is matched, not the
  substring, so `properties="mathml"` is unaffected. The guard deliberately does
  not ask which EPUB version the book is: in an EPUB 2 book the property is
  itself invalid, and a second defect on the same element is a reason for a human
  to look, not a licence to delete it faster.

  Declining also removes the item from the shared spine guard's arithmetic — that
  guard asks whether a reading order would survive every deletion these fixers
  *could* propose, so counting a deletion that will never happen would make it
  decline runs that are safe.

**Measured.** 2 books in the 171-book corpus, both the same shape: a conversion
left `cover-1.jpg`/`cover-2.jpg` declared beside the real, present cover
(`id="cover"` → `cover.jpeg`, which is what `<meta name="cover">` actually names).
On this corpus neither guard fires — the dangling items are images, so nothing in
the spine references them, and they are not the declared cover. Grepping every
content document, the NCX and the OPF confirms the manifest entry itself is the
**only** thing in either book that mentions them.

The **nav guard is not argued but measured**, and it is the reason this entry
grew a third decline clause. On the shared 94-book shelf (2026-08-05), one book
declares `<item id="toc-idm…" href="toc01.xhtml" properties="nav"/>` for a file
the container does not hold. Dropping it was the **only** finding epubsana
introduced anywhere on that shelf, under both epubveri 0.5.18 and 0.9.7 — so it
was epubsana's own defect, live since this fixer shipped in 0.4.0, and invisible
to every unit test because each one asks whether the *edit* is right rather than
whether the *book* ends up better. With the guard in place the shelf run
introduces nothing at all. The spine guard remains argued rather than
corpus-tested, and is covered by unit tests instead.

---

## OPF-049 — dangling spine itemref

**Finding.** `opf.spine.itemref_idref_not_in_manifest`. A `<spine>` entry names a
manifest id that doesn't exist: `<itemref idref="no-such-id"/>`.

**Fix** (`fix.spine_dangling_itemref`, ConfirmNeeded). Drop the `<itemref>`
element. Deletion only; no other spine entry is touched and the reading order of
everything that remains is unchanged.

**Why it's safe.** The entry is inert. There is no manifest item, therefore no
document, therefore nothing to render at that position — it is a pointer to a
hole, and as with the dangling manifest item there is no information anywhere in
the book about what it was supposed to name. Drop it or keep the error; there is
no better third option.

`ConfirmNeeded` for the same reason as its sibling: it is a deletion from the
reading order, and deletions get looked at.

**Why it does not collide with `fix.manifest_dangling_item`.** That fixer drops
the spine entries it orphans itself, so an obvious worry is the two fighting over
the same `<itemref>` — especially since epubsana plans every fix once, from the
original report, and never re-plans. They cannot collide, and the reason is worth
stating: this fixer only ever sees an `OPF-049` **from the original report**,
i.e. an `idref` that was already absent from the manifest before any fix ran. The
cascade fixer only ever touches `idref`s that *were* present at plan time (their
item exists — it is the item's file that is missing). The two sets are disjoint by
construction, so plan-once is sound here rather than merely lucky.

**When it declines.**

- If the OPF won't parse, or no `<itemref>` carries the reported `idref`.
- **If dropping it would leave `<spine>` with no children** — same invariant as
  the sibling fixer, same reason.

**Measured.** 0 books in the 171-book corpus, which carries no spine-level finding
at all; verified by injection only. It lands regardless of its own frequency
because `fix.manifest_dangling_item` needs the concept to exist and the invariant
to be shared — the two were specified as one unit.

---

## OPF-034 / RSC-005 — duplicate spine itemref

**Finding.** `opf.spine.duplicate_itemref`. The `<spine>` lists the same manifest
item twice — `<itemref idref="id43"/>` more than once — so a chapter appears twice
in the reading order. epubveri reports the `idref` in `params[0]`, at the
**later** occurrence. It shows up in tool-converted books (Kindle→EPUB especially),
where a conversion step appends an itemref that already exists.

**This finding has two ids, and the fixer dispatches on the `rule`.** epubveri
reports the identical condition as `OPF-034` in EPUB 2 and `RSC-005` in EPUB 3 —
version-scoped, because that is what each epubcheck fixture expects. The `rule` is
the same for both, which is exactly what the `rule` sub-code exists for: a fixer
keyed on `OPF-034` would silently do nothing on every EPUB 3 book. The proposal
therefore inherits its `addresses_id` from the message rather than hard-coding one.

**Fix** (`fix.spine_duplicate_itemref`, ConfirmNeeded). Keep the **first**
occurrence, drop the later ones. Deletion only; no attribute is rewritten.

**Why it's safe.** The duplicate carries no information the first occurrence
doesn't already carry: same `idref`, therefore same document. The reading order is
preserved exactly, because the first occurrence is where the document actually
belongs in the sequence — dropping a later copy removes a repeat, not a position.
The spine can never be emptied by this fix, since the occurrence it keeps is by
definition still there, so it needs no empty-spine guard (unlike its dangling
siblings above).

`ConfirmNeeded`: it is a deletion, and it changes what a reader sees — a chapter
stops appearing twice.

**When it declines.**

- **When the duplicate's `linear` disagrees with the first's.** Two entries with
  the same `idref` but different `linear` are not a duplicate in the sense that
  matters: the book is saying "this document sits in the reading order *and* is
  reachable out-of-line", which is a real authored intent, and deleting one
  destroys it. `linear` is compared **normalized** — an absent `linear` means
  `yes`, so `<itemref idref="x"/>` and `<itemref idref="x" linear="yes"/>` are
  the same entry and the fix still applies. If any duplicate of an `idref`
  disagrees, the whole group is declined rather than half-repaired: mixed
  `linear` means the author was doing something deliberate with that document,
  and epubsana is not the one to guess what.
- **When the duplicate carries an `id` that the package refines.** An
  `<itemref id="x">` can be the target of a `<meta refines="#x">`, so dropping it
  would orphan that metadata — a finding epubsana would have created itself.
  Declined; the same principle as the `RSC-001` cascade, but here the referent is
  metadata we have no mandate to rewrite.
- If the OPF won't parse, or fewer than two itemrefs carry the reported `idref`
  (a stale finding never deletes anything).

**Measured.** **0 of 171 books** in the reference corpus, which contains no
Kindle→EPUB conversions — the shelf structurally cannot see this defect class.
Reproduced by epublift on a real book outside it (a Kindle conversion of *Project
Hail Mary*), and cheap and provably safe, so it lands on that evidence rather than
on ours. The guards are argued and unit-tested, not corpus-tested.

---

## HTM-004 — obsolete or unrecognized DOCTYPE

Two `HTM-004` findings, one per EPUB version, share a repair section because they
are the same defect seen through each version's rules. Both carry **no `params`**
and a position at the DOCTYPE. The repairs are **surgical on the DOCTYPE only** —
no other byte of the document is read or rewritten — and both bound the DOCTYPE the
way epubveri now does (up to its own closing `>`, never a `[` elsewhere in the
body — the lesson of the upstream bracket bug).

### `htm.doctype.epub3_obsolete_public_id` (EPUB 3) — `fix.doctype_html5`, AutoSafe

**Finding.** An EPUB 3 (HTML5) content document's DOCTYPE contains a `PUBLIC`
identifier. HTML5 has exactly one legal doctype — `<!DOCTYPE html>` — so any
public/system identifier is obsolete.

**Fix.** Replace the whole DOCTYPE with `<!DOCTYPE html>`.

**Why it's safe.** `<!DOCTYPE html>` is the one correct HTML5 doctype, and a
doctype declares no content — reducing it changes nothing a reader sees and clears
the finding. The document's own markup is untouched.

**When it declines.** If the DOCTYPE carries an **internal subset** (`<!DOCTYPE
html PUBLIC … [ … ]>`) — those `[ … ]` declarations (entities, notably) may be in
use, and HTML5's doctype cannot carry them, so stripping to `<!DOCTYPE html>`
could break the document. That is not a doctype relabel, so the fixer leaves it
for a human. (Also declines if the DOCTYPE can't be located.)

### `htm.doctype.epub2_unrecognized_public_id` (EPUB 2) — `fix.doctype_xhtml11`, ConfirmNeeded

**Finding.** An EPUB 2 content document's DOCTYPE is **not** one of the two EPUB 2
recognizes: `-//W3C//DTD XHTML 1.1//EN` or the OEB 1.2 identifier. EPUB 2 requires
XHTML 1.1.

**This one is deliberately narrow, and the reason is the whole point.** The
recognized set is *only* XHTML 1.1. So this finding also fires on a document that
declares a **different, legitimate DTD** — XHTML 1.0 Strict/Transitional, a bare
HTML5 `<!DOCTYPE html>`, or an OEB variant. Relabeling such a document to XHTML 1.1
is **not** a safe rename: XHTML 1.0 permits constructs 1.1 removed (`name=` on
anchors — a common fragment-target idiom in old books — presentational attributes,
…), so stamping `1.1` on a 1.0 document can trade this finding for a fresh crop of
content-model errors. Proving the document is *already* valid 1.1 is the detector's
job, not ours, and we do not re-validate at plan time. So we do not guess a content
model.

**Fix.** Set the DOCTYPE's public (and system) identifier to the canonical
recognized XHTML 1.1 form **only when the existing identifier is clearly a
malformed XHTML 1.1 identifier** — its public-id text names XHTML 1.1, or its
system id is the `xhtml11.dtd` URL, but the exact recognized string is mistyped
(wrong whitespace, a missing slash). There the author's intent is unambiguous and
the canonical form is the one correct spelling.

`ConfirmNeeded`: it edits the declared document type, which a strict reader can act
on.

**When it declines — which on real books is the common case.** A DOCTYPE that
declares a *genuinely different* DTD (XHTML 1.0, bare `<!DOCTYPE html>`, OEB, or
nonsense) is left untouched and the finding stays reported: correcting it would
assert a content model epubsana can't verify. On the reference corpus the single
affected book is XHTML 1.0 Strict (77×) — **declined**, correctly. Also declines if
the DOCTYPE can't be located.

### What this means for the family claim

`htm.doctype` is **handled end to end** — every finding gets either a repair or a
principled decline — but it is *not* "every occurrence rewritten". The honest
public phrasing is: *epubsana normalizes obsolete EPUB 3 doctypes and canonicalizes
malformed XHTML 1.1 identifiers, and declines to relabel a document that declares a
different DTD (which would assert an unverified content model).* The decline is a
feature, not a gap: it is the same "never guess" rule that governs every fixer here.

---

## RSC-005 — NCX internal consistency

The NCX (the EPUB 2 table of contents) has a small, self-contained set of
internal-consistency rules, and epubsana now covers the whole determinate part of
it: **invalid NCName ids** and the **`dtb:uid` mismatch** (above), plus the two
below. NCX ids are **not IDREF targets anywhere in an EPUB** — nothing links into
an NCX by id — so making an id valid or unique never rewrites a reference, which is
what makes these repairs surgical.

### `ncx.ids.duplicate_id` — `fix.ncx_duplicate_id`, ConfirmNeeded

**Finding.** Two or more elements in the NCX carry the same `id`. epubveri reports
each offending element with the value in `params[0]`.

**Fix.** Keep the **first** occurrence of each duplicated id; rename every later
one to a fresh unique id (the value suffixed `-2`, `-3`, … until unique across the
NCX). Only the later occurrences change, so the first element keeps the id a
reader or tool might already know.

**Why it's safe.** An NCX id is a label, not a link target, so renaming a duplicate
introduces no dangling reference and the uniqueness suffix cannot collide with an
existing id (it is checked against them). The value is otherwise preserved.

**Disjoint from the NCName fixer, by construction.** `fix.ncx_ncnames` only touches
an id whose attribute occurs **exactly once** (so its surgical rewrite is
unambiguous); a duplicate occurs **more than once**. The two fixers therefore never
target the same id, and planning them once from the original report is sound.

**When it declines.** If the NCX text can't be read. (Any duplicate can be made
unique, so there is nothing else to decline.)

### `ncx.play_order.duplicate` — `fix.ncx_play_order`, ConfirmNeeded

**Finding.** Two navigation elements (`navPoint`/`navTarget`/`pageTarget`) carry the
same `playOrder` while pointing at **different** targets. epubveri reports the
repeated value in `params[0]`. (On the corpus this is the classic tool bug: every
element emitted with `playOrder="1"`.)

**Fix.** Renumber **every** `playOrder` in the NCX to its 1-based position in
document order (`1`, `2`, `3`, …). This is the canonical NCX assignment — `playOrder`
is defined to mirror document order — and it makes every value unique in one pass.

**Why it's safe.** `playOrder` is only a *hint*: the reading order a system actually
follows is the spine, which this fixer never touches. Renumbering to document order
can't mislead, because document order is exactly what `playOrder` is meant to
express. It is `ConfirmNeeded` because it rewrites values broadly — including
correct ones — and the change is visible. Elements that legitimately *shared* a
`playOrder` (same target — permitted, and not flagged) receive distinct numbers;
distinct is always valid.

**When it declines.** If the NCX text can't be read.

### `ncx.page_target.invalid_type` — declined (the family's one judgement member)

The third internal rule. A `pageTarget`'s `@type` must be `front`, `normal`, or
`special`; a bad value has **no single correct replacement** — we cannot know a
page's category from an invalid string, and `normal` is only a plausible default,
not a determinate answer. Setting one would be a guess, so epubsana **declines** it
and the finding stays reported (0 corpus cases). This is the same "never guess"
line that governs the different-DTD doctype decline: the family is *handled* — every
member is fixed where determinate and declined where it would require invention.

---

## RSC-007 / RSC-017 / RSC-012 — guide references

The EPUB 2 `<guide>` is a list of `<reference type="…" href="…"/>` pointers to
structural landmarks (cover, toc, text). All three defects here are cleared by
**deleting** something — a whole reference, a redundant repeat, or a fragment
that resolves nowhere. A guide reference is pure navigation, so removing a broken
or redundant pointer loses nothing a reader can reach. This closes the whole
`opf.guide` family (all three of its rules).

The family was "complete" at two rules until epubveri 0.9.16 added the third,
which is the ordinary way a closed family reopens: **a family is closed against
the rules that exist, not for good.**

### `opf.guide.reference_missing_resource` — `fix.guide_dangling_reference`, ConfirmNeeded

**Finding.** `RSC-007`. A `<guide>` reference's `href` doesn't resolve to any
resource in the container — on the corpus, typically a wrong extension
(`Text/rica.html` beside `rica.xhtml`). epubveri reports the `href` in `params[0]`.

**Fix.** Drop every `<reference>` whose `href` is one epubveri flagged. If that
would leave the `<guide>` with no references, drop the `<guide>` element itself
(an empty `<guide>` is invalid — OPS 2.0 requires `reference+` — and `<guide>` is
optional, so removing it is the correct resolution, not a new defect).

**Why it's safe.** The reference points at a resource that does not exist; as with
a dangling manifest item or spine itemref, it cannot be repaired *into* anything —
nothing in the book records what file it meant. A guide reference is not content;
dropping it removes a pointer to a hole, and every reference that still resolves
keeps its place. We match on the `href` epubveri reported and **do not re-resolve
paths** — whether an `href` resolves is the detector's call, not a second opinion
here.

**When it declines.** If the OPF won't parse, or no `<reference>` carries the
reported `href`.

### `opf.guide.duplicate_reference` — `fix.guide_duplicate_reference`, ConfirmNeeded

**Finding.** `RSC-017`, at warning severity. Two or more `<reference>` elements
share the **same `type` and the same `href`** — a redundant repeat. (References
with the same `type` but *different* `href`, e.g. several `type="text"` entries,
are **not** duplicates and are left alone.)

**Fix.** Keep the first reference of each identical `(type, href)` pair; drop the
later ones. Deletion only; nothing else in the guide moves.

**Why it's safe.** A second reference with an identical type and href carries no
information the first doesn't — it names the same landmark at the same target.
Removing it cannot change what any landmark resolves to. It cannot empty the guide
(the first of each pair is kept), so no empty-guide guard is needed.

**When it declines.** If the OPF won't parse, or fewer than two references share a
`(type, href)` (a stale finding never deletes anything).

### `opf.guide.reference_fragment_not_defined` — `fix.guide_dangling_fragment`, ConfirmNeeded

**Finding.** `RSC-012`, new in epubveri 0.9.16. A `<guide>` reference's `href`
carries a `#fragment`, the **document it names exists**, and the fragment resolves
to no `id` in it. epubveri reports `params[0]` = the fragment, `params[1]` = the
resolved path of the target document. It is the guide-side sibling of
`opf.ncx.content_fragment_not_defined`.

The detector has already excluded the cases that are not this defect: an empty
fragment (which addresses the document itself), a fragment carrying `=`, `:` or
`(` (a CFI or media fragment, not an id), and a target that could not be read or
parsed — where whether the fragment resolves is *unknown*, and epubveri says
nothing rather than guessing. So a finding that arrives here means the target
parsed cleanly and genuinely does not contain that `id`.

**Fix.** Drop the `#fragment`, keeping the `href`'s path. The reference goes on
naming the same document; it stops claiming a position inside it.

    <reference type="toc" href="Text/ch1.html#filepos16691"/>
    →
    <reference type="toc" href="Text/ch1.html"/>

**Why it's safe.** This is the one repair in the family that deletes nothing a
reader could otherwise reach, because **the behaviour is already what the repair
writes down.** A fragment that resolves to no `id` does not take a reading system
anywhere: it opens the document and lands at the top — exactly where the
fragment-less href lands. The edit makes the file state what already happens, and
the author's real choice, *which document is the landmark*, is preserved
untouched.

The corpus case is the shape that produces this: a `filepos…` anchor left behind
by a MOBI conversion, pointing into a document that a later split rewrote to hold
no ids at all. Nothing in the book records the position it meant, so there is
nothing to recover — and inventing a target `id`, or picking the "nearest" one,
would be guessing at the author's intent, which this project does not do.

Two repairs were considered and rejected. **Dropping the whole `<reference>`**
(what the two sibling fixers do) is wrong here: the target document exists, so the
landmark is real and only its sub-position is broken — deleting it would throw
away working navigation to fix a dangling anchor. **Retargeting the fragment to
some other `id`** is inventing.

**When it declines.**

- The OPF won't parse, or no `<reference>` carries the exact reported `href`.
- **Dropping the fragment would collide with another reference.** If the
  post-edit `(type, href)` pair equals that of any other reference in the same
  guide, the edit would clear an `RSC-012` and create an `RSC-017`
  (`opf.guide.duplicate_reference`) — a fix that leaves the book no better. That
  reference is left alone; any other flagged reference in the same guide is still
  repaired. Collisions are checked against the **whole post-edit guide**, so two
  flagged references that would become identical to *each other* are both
  declined, not silently merged.

**Note on `OPF-031`.** A dangling guide reference is often co-reported as `OPF-031`
("not declared in the manifest"), which carries no `rule` sub-code. Dropping the
reference clears that finding too, as a side effect — but epubsana keys only on the
`opf.guide.*` rules; the case where a guide reference names a file that *exists but
isn't in the manifest* (`OPF-031` alone, no `RSC-007`) is a different defect —
adding it to the manifest vs. dropping the reference is a judgement — and is not
part of this family.

---

## RSC-005 — a legacy `name` attribute on `<a>`

**Finding.** `htm.obsolete_attribute`, `params[0]` = the attribute's name, with
the message *attribute "name" not allowed here*. epubveri reports every obsolete
attribute through this one rule — `<br clear>`, other presentational leftovers,
and the pre-XHTML-1.1 `<a name="…">` anchor. **Only the anchor has a determinate
repair**, so this fixer is deliberately one member of the family wide.

**Fix** (`fix.anchor_name`, AutoSafe). In each affected document, for every `<a>`
carrying **both** `name` and `id` with the **identical value**, delete the `name`
attribute along with the whitespace that separated it from its neighbour. One
proposal per document; the element, its text, its other attributes and every
byte outside the attribute's own span are untouched.

**Why it's safe.** `name` on `<a>` was how a link target was declared before
`id` existed; XHTML 1.1 removed it, and epubcheck rejects it. Where the element
already carries an `id` with the same value, the anchor is *already* declared the
modern way — the two attributes are saying the same thing, and `#fragment` links
resolve through the `id`. So nothing that referenced the anchor moves, nothing
becomes unreachable, and no reading system renders anything differently. This is
the rare deletion that loses no information at all, which is what makes it
`AutoSafe`.

**When it declines.**

- **Any other attribute in the family.** `<br clear>` (10 findings on the shelf)
  is presentational and has no single markup equivalent — replacing it would mean
  choosing a CSS rule on the author's behalf.
- **`name` with no `id`.** The obvious repair — rename `name` → `id`, which
  preserves every `#fragment` that targets it — is *not* determinate. An `id`
  must be a valid NCName and must be unique in the document; a legacy `name` is
  under neither constraint (`name="1"` is legal where `id="1"` is not, and two
  anchors may share a name). Renaming can therefore manufacture a fresh finding,
  which is the one thing a repair must never do. 0 cases on the shelf; if a real
  book produces them, the branch gets specified then, with the NCName and
  collision tests spelled out.
- **`name` and `id` present but different.** Dropping `name` would break any
  `#fragment` that targets the name, and an element cannot carry two ids. There
  is no repair that keeps both, so this is a human's call.
- A document that doesn't parse, or in which no `<a>` matches (a stale finding
  never deletes anything).

**Measured.** 162 findings in **one** book on the 94-book shared shelf
(2026-08-05), and every one of them is the `id == name` shape —
`<a href="…#footnote-600-1" id="x" name="x">`, an annotation toolchain's output.
epubcheck reports the same defect with the same wording and the two tools' totals
on that book agree exactly (197 vs 197), so this is a closed epubcheck gap rather
than a divergence to be careful of. **One book is thin evidence**: it establishes
that the shape exists and that the repair is right for it, not that the shape is
representative. The declines above are what make that acceptable — an unfamiliar
shape is left alone rather than guessed at.

---

## RSC-005 — an empty `lang` / `xml:lang`

**Finding.** `opf.content_document.schema_violation`, message *value of attribute
"lang" is invalid: ""*, `params` = `[attribute, value]`. EPUB 2's grammar types
`lang` and `xml:lang` as a language tag, and the empty string is not one. (EPUB 3
is HTML5, where `lang=""` legally means "undetermined" — so this only ever arises
on EPUB 2, from the grammar rather than from a version test here.)

As with the stray-text fixer, `schema_violation` is one rule over a whole grammar,
so the match takes the message prefix (*value of attribute*) **and** `params`:
`params[0]` ∈ {`lang`, `xml:lang`} and `params[1]` empty.

**Fix** (`fix.empty_lang`, ConfirmNeeded). Delete the attribute, with the
whitespace that separated it. One proposal per document; `lang` and `xml:lang` on
the same element go together in it, since a document that has one almost always
has the other (140 and 140 on the shelf).

**Why it is `ConfirmNeeded` and not `AutoSafe`.** The deletion looks inert — an
empty language tag names no language — but it is not. `<p lang="">` inside
`<html lang="tr">` currently declares *undetermined*; with the attribute gone the
paragraph inherits `tr`, and a reading system acts on that: hyphenation, the
text-to-speech voice, font selection for CJK. XHTML 1.1 offers no valid way to
spell "undetermined", so the real choice is between an invalid document and one
that inherits its parent's language. That is a decision about the book, and the
caller should make it — the alternative repair (guessing the intended language)
is exactly the invention epubsana refuses.

**When it declines.**

- **A non-empty invalid value.** `lang="en_US"`, `lang="turkish"` — these are
  malformed rather than absent, and repairing them means guessing which tag was
  meant (`en-US`? `tr`?). 0 non-empty cases on the shelf.
- Any other attribute reported through the same message shape — notably
  `value of attribute "id" is invalid` (312 findings, 5 books), which is a
  *cross-file* repair: renaming an id means moving every `href="#…"` that targets
  it, in every document plus the NCX and the OPF, without colliding with an
  existing id. Determinate in principle, not a local edit, and not this fixer.
- A document that doesn't parse, or in which no attribute matches.

**Measured.** 280 findings (140 `lang` + 140 `xml:lang`) in **one** book on the
94-book shelf, all empty-valued. The same caveat as the anchor fixer applies: one
book shows the shape is real and the repair right for it, and the declines carry
the weight for everything else.

---

## OPF-054 — an empty `<dc:date>` (EPUB 2)

**Finding.** `OPF-054`, **no `rule`**, reported on the package document at the
offending element: *dc:date value '' is empty or doesn't conform to ISO 8601*.
Contributed as a requirement by `epublift`, which carried its own repair for it
([#5](https://github.com/veripublica/epubsana/issues/5)).

**Read the emission site before believing the id.** Two things about it decide
the whole shape of this fixer:

- **It is EPUB 2 only.** epubveri runs one check (`is_valid_dc_date`) and splits
  by version: `OPF-054`/Error on EPUB 2, **`OPF-053`/Warning on EPUB 3**. The same
  version-scoped split as the duplicate spine itemref, and the same lesson —
  except here it cuts the other way. An EPUB 3 book with a broken date is not
  invalid, so there is nothing for a `--goal valid` repairer to clear.
- **The trigger is not "empty".** It is "not a valid W3C-DTF date", which covers
  an empty value *and* a malformed one, under one id and one message. That
  distinction is this fixer's entire content: the two need opposite treatment.

**Fix** (`fix.empty_dc_date`, ConfirmNeeded). Drop every `<dc:date>` child of
`<metadata>` whose text content is empty or whitespace-only, with the whitespace
that preceded it. One proposal per package document.

**Why it is safe.** An empty element states nothing: there is no date in it to
lose, and `dc:date` is optional in EPUB 2 (only `dc:title`, `dc:identifier` and
`dc:language` are required), so its absence is valid. The invariant carried over
from epublift's version — *only drop an empty element if a non-empty sibling of
the same required type survives* — is kept, and is vacuous here precisely because
`dc:date` is not one of the required types.

**Why `ConfirmNeeded` and not `AutoSafe`.** It is a deletion of authored markup,
and an empty `<dc:date>` is a statement that a date was *meant* to be here.
Removing it is safe for validity and still destroys the only evidence the book's
date is missing rather than never-intended. That is the caller's call.

**Filling it instead is out of scope, deliberately.** The natural comparison is
`empty_title`, which fills rather than drops — but that fixer moves text the
author already wrote from one part of the container to another. A publication
date is not in the book; getting one means asking a third party over a network,
which is asserting a fact about the world and writing it into someone's file.
That is outside epubsana's contract (repair what epubveri reports, using only
what is in the container). If enrichment happens it belongs to the orchestrator,
which then hands us a `<dc:date>` that isn't empty and we propose nothing.

**When it declines.**

- **Any non-empty value — and this is the majority of what `OPF-054` reports.**
  `2022-09-08)`, `March 2019`, `2019/10/31` are malformed but they carry a real
  authored date. Dropping one destroys information the book actually has, and
  repairing one means deciding which characters are stray or parsing natural
  language — a guess either way. Left untouched, so the finding survives the
  repair, which is the honest outcome.
- **A `<dc:date>` carrying an `id` that a `<meta refines="#…">` targets.**
  Dropping it would orphan the refinement and trade this finding for another.
- A package document that doesn't parse, or in which no `<dc:date>` is empty
  (including the case where the finding is real but describes a malformed value).

**Measured.** **Zero** occurrences on the 94-book shelf, and zero on the earlier
171-book corpus — injection-only on both sides, which is why it ranked last of
the four fixers `epublift` handed over. What the shelf does hold is one
`OPF-053`: an EPUB 3 book whose date reads `2022-09-08)`. That is the declining
case, on the id this fixer does not act on — so the single real-world specimen we
have is evidence for the decline, not for the repair. The guards are covered by
unit tests, not by real books, and that should be said plainly whenever this
fixer is claimed.

---

## RSC-005 — an `id` that is not a valid NCName (the first cross-file fixer)

**Finding.** `opf.content_document.schema_violation`, message *value of attribute
"id" is invalid: "…"*, `params` = `[attribute, value]` with `params[0] == "id"`.
Like the stray-text and empty-`lang` fixers, this reaches through one rule that
covers a whole grammar, so the match takes the message prefix **and** `params`.

**One defect, measured.** All **312** findings on the 94-book shelf are the same
thing: an id that **starts with a digit**, which an XML NCName may not. Zero are
invalid for any other reason. So the repair is the sanitize-and-rename the NCX
fixer already does — `id_09`, made unique against the ids already in the
document.

**What makes it different from `ncx_ncnames`, and why it is the first cross-file
fixer.** That fixer renames freely because NCX ids are not reference targets.
These are: **191 of the 312 are pointed at** — 181 times from the NCX, 150 from
other content documents, twice from the OPF. A rename that leaves a reference
behind trades this finding for a dangling fragment, which is precisely the
self-inflicted regression the house rules care most about.

**Fix** (`fix.content_document_invalid_id`, ConfirmNeeded). One proposal per
document, covering every invalid id in it. Each id is sanitized and made unique,
and **every reference to it is rewritten in the same edit**: fragments inside the
document itself, links from other content documents, and the NCX's
`<content src="…#…"/>`.

**How a reference is attributed, and why it must be.** The hazard here is
measured, not hypothetical: **six values on the shelf are carried by 6–12
different documents of the same book**, so a global search for `#value` would
rewrite links that legitimately mean *another* document's identically-named id.
Every occurrence is therefore resolved — the path part of the attribute value it
sits in is resolved against the **referring file's own directory** (treating it
as container-absolute is the mistake `frag_diag` made once and `docs/API.md`
records), and rewritten only when it lands on this document. A bare `#value`
belongs to the file it appears in.

**When it declines.**

- **An occurrence it cannot classify.** A `#value` that is not inside a quoted
  attribute value — an id selector in a stylesheet, a string in script, a mention
  in prose — cannot be rewritten with confidence, so the whole rename for that id
  is abandoned. This is the safety net, and it is why the fixer can promise that
  a rename never leaves a reference behind.
- **A percent-encoded path** in a reference: we do not guess at an encoding we
  would then have to re-emit.
- **Two elements sharing the invalid id.** That is a duplicate-id defect, and
  which of them a reference meant is not ours to guess.
- **A value nothing usable survives sanitizing.** Never invent an id.

**One filter that is correctness, not optimization.** Only markup, styles and
script are scanned for references (`.xhtml`, `.html`, `.htm`, `.xml`, `.ncx`,
`.opf`, `.svg`, `.css`, `.js`). `Workspace::get_text` will hand back the bytes of
a JPEG as a string, and a cover image reliably contains a byte pair spelling
`#1`; that occurrence is unclassifiable, and since unclassifiable means decline,
**one cover image was enough to abandon ten perfectly repairable ids on a real
book** before this filter existed. It was caught by the shelf, not by a test.

**Measured.** 312 findings across **5 books**; after the fix **all five hold zero
invalid ids**. One more book reaches fully valid (3 → 4 on the shelf), and both
whole-shelf instruments — `regression_audit` and the round-trip check, the latter
comparing ID sets as well as counts — report **nothing introduced anywhere**.
That is the strongest evidence any fixer here has shipped with, and it is the
right bar for the first one that edits more than one file at a time.

---

## RSC-005 — an EPUB 3 attribute on an EPUB 2 package document

**Finding.** `opf.package.schema_violation`, message *attribute "…" is not allowed
here*, `params` = `[attribute]`, reported on the package document. The OPF's own
grammar, not a content document's.

**Do not confuse it with its content-document twin.** `attribute "X" is not
allowed here` under `opf.content_document.schema_violation` is the largest and
least repairable surface epubveri reports (and the one it is still changing), and
`docs/COVERAGE.md` says to stay away from it. This is a different rule over a
much smaller vocabulary: the package document has a handful of attributes, EPUB 2
and EPUB 3 differ in a knowable way, and there are 4 findings on the shelf, not
thousands. The rule name is the discriminator — the message text alone is not.

**The defect.** A book declares `version="2.0"` but carries an attribute EPUB 3
introduced. On the shelf it is exactly two, and **both turn out to assert nothing
the book does not already say**:

    3 books  <item href="…jpg" id="cover-image" … properties="cover-image"/>
             <meta name="cover" content="cover-image"/>   ← the EPUB 2 declaration,
                                                            naming that same item
    1 book   <spine toc="ncx" page-progression-direction="ltr">
                                                    ↑ EPUB 3's own default value

**Fix** (`fix.epub3_attr_in_epub2_package`, AutoSafe). Delete the attribute, with
the whitespace that preceded it — but **only after verifying, in that book, that
it carries no information**:

- `properties="cover-image"` on a manifest item is dropped **iff** the package
  also has `<meta name="cover" content="…">` naming *that item's own `id`*. Then
  the cover is already declared the way EPUB 2 declares it, at the same item, and
  the attribute is pure redundancy.
- `page-progression-direction` is dropped **iff** its value is `ltr`. That is the
  default in EPUB 3 too, so the attribute asserts nothing anywhere.

**Why `AutoSafe`.** Deletions are normally `ConfirmNeeded` here, because they can
shorten a reading order or remove a cover declaration. This one provably cannot:
each case is checked to be redundant *before* it is proposed, and the check is
about this book rather than about the shape in general. That is the same standard
that made the legacy `<a name>` fixer `AutoSafe` — a duplicate declaration whose
information demonstrably survives its own removal.

**When it declines.**

- **Any other `properties` token** — `nav`, `mathml`, `scripted`, `svg`,
  `remote-resources`, `switch` — or more than one token. EPUB 2 has no equivalent
  declaration for these, so dropping one would silently discard a real claim
  about the document rather than a redundant one.
- **`properties="cover-image"` with no matching `<meta name="cover">`**, or one
  that names a different item. Then the attribute is the *only* cover
  declaration, and removing it would lose the cover.
- **`page-progression-direction="rtl"`** (or any value that is not `ltr`). A
  right-to-left reading order is real authored information. EPUB 2 has nowhere to
  put it, which is a reason to leave the book alone, not a reason to erase it.
- **Any other attribute name** reported through this rule, and a package document
  that doesn't parse.

**Measured.** 4 findings across **4 books**, and the fix clears all four — the
rule goes to zero on the shelf. Both whole-shelf instruments report nothing
introduced. Note what the declines cost here: **nothing on this shelf**, because
no book carries the shapes they guard against. They are written from the
specification rather than from the corpus, and should be described that way.

---

## RSC-005 — a duplicate `id` in a content document

**Finding.** `opf.content_document.duplicate_id`, message *Duplicate ID "…"*,
with the value in `params[0]`. Two or more elements in one content document carry
the same `id`, which XML forbids outright.

**Fix** (`fix.content_document_duplicate_id`, ConfirmNeeded). The **first**
occurrence in document order keeps the id; every later one is renamed to a unique
value (the same value suffixed `-2`, `-3`, … until it collides with nothing in
the document). One proposal per document.

**Why no reference moves — and this is the whole argument.** Content-document ids
*are* reference targets, unlike the NCX ids of the sibling fixer, so "rename and
touch nothing else" needs justifying rather than asserting. It holds because a
fragment reference into a document with a duplicated id already resolves to the
**first** element in tree order with that id; that is what every conforming
processor does, and it is what a reader has been seeing. Keeping the first
occurrence therefore leaves every `#fragment` pointing at exactly the element it
already pointed at. Renaming the *first* and moving references would be the
riskier repair for no gain.

On the shelf the point is moot twice over: **none of the 21 duplicated ids is
referenced from anywhere in its book**. So the reasoning carries this fixer, not
the corpus — say it that way.

**Where it sits next to the invalid-id fixer.** They are disjoint by
construction. `fix.content_document_invalid_id` renames an id it can prove occurs
**exactly once**, and declines a duplicated one precisely because which element a
reference meant would then be a guess. This fixer handles the other case. Where a
value is *both* duplicated and not a valid NCName, the new names are built from
the sanitized stem, so the repair cannot manufacture more invalid names than it
found; the surviving first occurrence stays for the other fixer to consider on a
later run. Zero such cases on the shelf — the guard is reasoning, not measurement.

**When it declines.**

- **A stale finding**: the reported value no longer occurs twice in the document
  (an earlier approved fix already moved it). Nothing is renamed.
- A document `Workspace` cannot read.

**Measured.** 53 findings across **21 distinct ids in one book**, ids repeating
two to four times each, zero references to any of them. One book is thin
evidence, and the honest claim is the mechanism rather than the coverage: it
clears every duplicate-id finding on the shelf and introduces nothing.

---

## RSC-007 — a reference whose path is wrong but whose target is in the book

**Finding.** `opf.content_document.reference_missing_resource`, message
*reference to a resource missing from the publication: '…'*, with the reference
exactly as written in `params[0]`.

**This rule was investigated on 2026-08-06 and closed as human-only.** At the
time all nine findings on the 94-book shelf were scheme-less bare hostnames
(`www.youtube.com/watch?v=…`) or placeholder junk (`XXXXXXXX…`), and neither has
a determinate repair. **The shelf grew to 115 books and brought a different
shape**, which is the whole reason this entry exists — a rule closed on one
corpus is not closed for good, and re-probing a "dead" rule after the corpus
moves is cheap.

The new shape is a **real internal link with a stale path**:

    document 1/Bolum013.xhtml links to  ../Text/DiPNOTLAR.xhtml#a8
    the file actually lives at          1/DiPNOTLAR.xhtml

A book restructured after it was authored — the `../Text/` prefix survived the
move. The target is still in the container, under the same name, one directory
away.

**Fix** (`fix.reference_wrong_path`, ConfirmNeeded). Rewrite the **path portion**
of the reference to point at the entry that carries it, expressed relative to the
referring document's own directory. The **fragment is carried across untouched**,
and nothing else in the document changes.

**Why it is determinate.** The reference names a file by basename, and exactly
one entry in the container has that basename — so that entry is the file it
meant. There is no candidate set to choose from and nothing is invented.

**The fragment is a guard and a corroboration at once.** Rewriting a path could
trade `RSC-007` for a dangling `RSC-012` if the fragment does not exist in the
new target, so the fixer checks that it does before proposing. That check also
does evidential work: a same-named file that merely *happened* to be elsewhere in
the container would not contain `#a8`. Measured: **all 24 shelf cases have their
fragment present in the target**, so on this corpus the two agree every time.

**When it declines.**

- **The basename matches nothing** in the container (2 findings). The file is
  genuinely absent; a repair would have to invent it.
- **The basename matches more than one entry.** Which one the reference meant is
  then a guess, and a wrong guess is a silently broken link rather than a visible
  error. Zero cases on the shelf; the guard is reasoning.
- **The fragment is not in the chosen target.** Clearing this finding by creating
  a dangling one is not a repair.
- **Anything that is not a container path**: an absolute URL, a scheme-less bare
  hostname (6 findings — `www.dogankitap.com.tr` and friends, where the repair
  would be to *assert* the author meant an external URL), and placeholder junk
  (4 findings, `XXXXXXXX…`). These are the shapes that closed this rule the first
  time, and they are still declined.
- A percent-encoded path, on the same principle as the invalid-id fixer: we do
  not guess at an encoding we would then have to re-emit.
- The exact reference not being findable as a quoted attribute value in the
  document — the fixer edits what it can see, and goes quiet otherwise.

**Measured.** 36 findings across 8 books, of which **24 in 3 books** are the
repairable shape and clear completely; the other 12 are declined by the rules
above. Both whole-shelf instruments report nothing introduced.

---

## OPF-030 / RSC-005 — the package's declared identifier points at nothing usable

**Findings.** Two rules, one defect seen at two stages:

- `opf.package.unique_identifier_unresolved` (`OPF-030`) — `<package
  unique-identifier="X">` names an id no `<dc:identifier>` carries.
- `opf.package.opf_identifier_not_empty` (`RSC-005`) — the `<dc:identifier>` that
  *does* carry `id="X"` is empty.

Either way the package declares which identifier is canonical and that
declaration lands on nothing a reading system can use. On the then-125-book shelf (2026-08-09) the
two rules hit **disjoint sets of five books each**.

**Fix** (`fix.package_identifier`, ConfirmNeeded). Make the declaration point at
the identifier the book actually has — **when there is exactly one candidate**:

- *Unresolved*: give `id="X"` to the single `<dc:identifier>` that has no `id`
  and a non-empty value.
- *Empty*: give `id="X"` to the single non-empty sibling that has no `id`, and
  drop the now-redundant empty element with the whitespace before it.

**Why it invents nothing.** The value is already in the book, written by its
producer; the id is already in the book, written in the `unique-identifier`
attribute. The repair only attaches the one to the other. Compare `empty_title`,
which moves a TOC label the author wrote — same principle, different pair.

**Why not the alternatives.** Copying a sibling's value *into* the empty element
would leave two `<dc:identifier>`s asserting the same string; repointing
`unique-identifier` at some other identifier would be choosing which of the
book's identities is canonical. Moving the declared id onto the sole real
identifier does neither.

**It carries the NCX with it, in the same proposal.** Making the package
identifier resolvable is what first lets epubveri compare the NCX's `dtb:uid`
against it — so on three shelf books the first cut of this fixer *unmasked* a
pre-existing mismatch and produced `NCX-001` where there had been none. A
whole-shelf audit caught it; no unit test could have, because the edit was
correct and the *book* ended up worse. The `dtb:uid` is therefore synced to the
same value in the same edit, on the pattern
[`fix.manifest_dangling_item`](#rsc-001--dangling-manifest-item) already sets:
approving half of this would leave a finding epubsana created itself.

**When it declines — and it declines on 7 of the 10 books.**

- **More than one candidate.** Four books carry both a UUID and an ISBN with no
  `id` on either. Both are legitimate publication identifiers and **which one is
  canonical is an editorial decision**, not a repair. The attribute's *name*
  hints (`uuid_id`), and hints are not evidence.
- **No `<dc:identifier>` at all** (two books). The repair would have to generate
  one — a UUID from nowhere — which is the invention this project does not do.
  Note the standing question in `CLAUDE.md` about generated identifiers is
  therefore still unanswered and still not needed.
- **The single candidate is itself empty.** Attaching the id would clear
  `OPF-030` and raise the empty-identifier finding in its place, which is not a
  repair.
- A package document that doesn't parse.

**Measured.** 10 findings across 10 books; **3 repaired, 7 declined**. That ratio
is the honest headline: this fixer's value is that it refuses to guess about a
book's identity six times out of ten, and the three it does repair need no guess
at all.

---

## RSC-005 — the three `playOrder` faults (an addendum to NCX internal consistency)

epubveri reports three separate `playOrder` rules and they interlock, so
satisfying one naively breaks another:

- `ncx.play_order.duplicate` — **different** targets sharing a number.
- `ncx.play_order.target_mismatch` — **one** target reached by elements carrying
  different numbers.
- `ncx.play_order.gap` — a number with no predecessor.

`fix.ncx_play_order` now reassigns the whole NCX the way the format defines:
**1-based, dense, in document order, and elements naming the same target share
the first number that target was given.** That satisfies all three at once, and
no assignment satisfying all three differs from it.

**Why it was rewritten, and the defect it removes.** The first version numbered
every `playOrder` by its position in the file — unique, dense, and *target-blind*.
On a book whose navigation reaches one position by two routes it would have
**created** `target_mismatch`. No shelf book had that shape, so no audit could
show it; the defect surfaced by reading epubveri's own rule, which skips a
repeated number when all its holders name the same target ("one position, reached
by several routes — legitimate"). This is the class of defect a corpus cannot
find, and reading the detector's source can.

**A behaviour change worth naming:** the renumbering now **parses** the NCX
instead of scanning it for `playOrder=`, because a target cannot be read off a
string. An NCX that will not parse is therefore declined rather than rewritten —
consistent with every other structural fixer here, and stricter than before.

**Measured.** 14 `duplicate` + 8 `target_mismatch` + 1 `gap` cleared across the
shelf, nothing introduced, and one more book reaches fully valid (5 → **6**).

---

## RSC-005 — an anchor target wrapped around a link

**Finding.** `htm.epub2_dom.nested_anchor`, message *The "a" element cannot
contain any nested "a" elements*. `params` is empty, so the finding gives the
file and nothing else; the element is re-located by parsing, as the other
structural fixers do.

**The shape, and why it is determinate.** On the shelf every case is the same
thing — a footnote reference where the *outer* `<a>` carries no `href`:

    <a id="bookmark1"><sup><a href="#footnote1">1</a></sup></a>

The outer element is not a link at all; it is an **anchor target**, the legacy
way of naming a position before every element could carry an `id`. XHTML forbids
nesting anchors, and the `id` does not need an `<a>` to live on.

**Fix** (`fix.nested_anchor`, ConfirmNeeded). Unwrap the outer `<a>` and move its
`id` onto its single element child:

    <sup id="bookmark1"><a href="#footnote1">1</a></sup>

`#bookmark1` still resolves, to an element at the same place in the same
rendered line. Nothing is deleted but a wrapper that carried no information of
its own.

**When it declines.**

- **The outer `<a>` has an `href`.** Then it is a real link, and unwrapping it
  destroys a navigation the author wrote. Which of two nested links to keep is
  not ours to decide.
- **The outer `<a>` carries any attribute other than `id`** — a `class`, a
  `style`, an `epub:type`. Those would be lost, and moving them onto a different
  element asserts they apply to it.
- **The single child already has an `id`.** Two ids cannot share one element and
  choosing between them is not a repair.
- **More than one element child, or non-whitespace text beside the child.** The
  `id` would then have to be attached to something that covers less than the
  anchor did.
- A document that will not parse.

**Measured.** 6 findings in **one** book, all the footnote shape above, all
repaired. One book is thin evidence and the honest claim is the mechanism: it
moves an id off a wrapper that exists only to hold it, and refuses every case
where the wrapper carries anything else.
