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
| `RSC-005` | `opf.content_document.empty_title` | ConfirmNeeded | An XHTML `<title>` element is empty | [Fill it from the book's own TOC label, else its first heading, else the book's title](#rsc-005--empty-title) |
| `RSC-005` | `opf.content_document.schema_violation` (`incomplete_content`, `params` = `["head", "title"]`) | ConfirmNeeded | An XHTML `<head>` has no `<title>` at all | [Add one, from the same three sources](#rsc-005--a-head-with-no-title) |
| `RSC-020` | `opf.manifest_item.unencoded_space_in_href` | AutoSafe | A manifest `href` contains a raw space | [Percent-encode the space as `%20`](#rsc-020--unencoded-space-in-a-manifest-href) |
| `RSC-020` | `opf.ncx.content_src_unencoded_space` | AutoSafe | An NCX `<content src>` contains a raw space | [Percent-encode the space as `%20`](#rsc-020--unencoded-space-in-an-ncx-content-src) |
| `RSC-020` | `opf.content_document.malformed_relative_url` (a space) | AutoSafe | A link or image reference in a content document contains a raw space | [Percent-encode the space, where the validator then accepts the URL](#rsc-020--unencoded-space-in-a-content-documents-url) |
| `OPF-014` | `opf.content_document.property_used_undeclared` | AutoSafe | A content document uses a feature its manifest item doesn't declare | [Add the token to that item's `properties`](#opf-014--undeclared-content-property) |
| `PKG-006` / `PKG-005` | *(none)* | AutoSafe | The `mimetype` entry is not first in the ZIP, or its header carries an extra field | [Re-emit it first and stored, touching no content](#pkg-006--pkg-005--the-mimetype-entrys-packaging) |
| `PKG-014` | *(none)* | AutoSafe | A ZIP directory entry has nothing inside it | [Leave the entry out of the archive](#pkg-014--an-empty-directory-entry) |
| `RSC-005` | `opf.content_document.schema_violation` (stray text / inline element / incomplete content, in `<body>` or `<blockquote>`) | ConfirmNeeded | EPUB 2 text or inline elements sit where the grammar requires block content | [Wrap each run in one `<div>`, leaving whitespace alone](#rsc-005--non-block-content-in-body-or-blockquote-epub-2) |
| `RSC-001` | `opf.manifest_item.missing_resource` | ConfirmNeeded | A manifest `<item>` declares a resource the container doesn't hold | [Drop the item, and every reference that named it](#rsc-001--dangling-manifest-item) |
| `OPF-049` | `opf.spine.itemref_idref_not_in_manifest` | ConfirmNeeded | A spine `<itemref>` names a manifest id that doesn't exist | [Drop the itemref](#opf-049--dangling-spine-itemref) |
| `OPF-034` / `RSC-005` | `opf.spine.duplicate_itemref` | ConfirmNeeded | The spine lists the same manifest item more than once | [Keep the first occurrence, drop the later ones](#opf-034--rsc-005--duplicate-spine-itemref) |
| `HTM-004` | `htm.doctype.epub3_obsolete_public_id` | AutoSafe | An EPUB 3 document's DOCTYPE carries an obsolete PUBLIC identifier | [Reduce it to `<!DOCTYPE html>`](#htm-004--obsolete-or-unrecognized-doctype) |
| `HTM-004` | `htm.doctype.epub2_unrecognized_public_id` | ConfirmNeeded | An EPUB 2 document's DOCTYPE isn't a recognized XHTML 1.1 / OEB identifier | [Canonicalize a malformed XHTML 1.1 id; decline a genuinely different DTD](#htm-004--obsolete-or-unrecognized-doctype) |
| `RSC-005` | `ncx.ids.duplicate_id` | ConfirmNeeded | Two or more NCX elements share an `id` | [Keep the first, rename later duplicates uniquely](#rsc-005--ncx-internal-consistency) |
| `RSC-005` | `ncx.play_order.duplicate`, `…target_mismatch`, `…gap`, `…no_origin` | ConfirmNeeded | The NCX's `playOrder` values are inconsistent | [Reassign densely by document order, same target → same number](#rsc-005--ncx-internal-consistency) |
| `RSC-005` | `opf.content_document.lang_xmllang_mismatch` | AutoSafe | `lang` and `xml:lang` disagree on one element | [Fill the empty one from the populated one; decline if both are set](#rsc-005--lang-and-xmllang-disagree) |
| `RSC-007` | `opf.ncx.content_src_missing_resource` | ConfirmNeeded | An NCX `<content src>` path no longer resolves | [Repoint it when exactly one entry carries the basename](#rsc-007--an-ncx-content-src-whose-path-no-longer-resolves) |
| `RSC-007` | `opf.guide.reference_missing_resource` | ConfirmNeeded | A `<guide>` reference points at a resource that doesn't exist | [Drop the reference; drop the guide if it empties](#rsc-007--rsc-017--rsc-012--guide-references) |
| `RSC-017` | `opf.guide.duplicate_reference` | ConfirmNeeded | Two `<guide>` references share a `type` and `href` | [Keep the first, drop the duplicates](#rsc-007--rsc-017--rsc-012--guide-references) |
| `RSC-012` | `opf.guide.reference_fragment_not_defined` | ConfirmNeeded | A `<guide>` reference's `#fragment` resolves to no `id` in a target that does exist | [Drop the fragment, keep the document](#rsc-007--rsc-017--rsc-012--guide-references) |
| `RSC-012` | `opf.content_document.dangling_fragment` | ConfirmNeeded | A link's `#fragment` is real but now lives in another document — an editor split the file | [Write the path in front of the fragment](#rsc-012--a-fragment-that-is-real-but-has-moved-to-another-document) |
| `RSC-005` | `htm.obsolete_attribute` (`params[0] == "name"`) | AutoSafe | A legacy `<a name>` anchor duplicating the element's own `id` | [Drop the `name` attribute](#rsc-005--a-legacy-name-attribute-on-a) |
| `RSC-005` | `opf.content_document.schema_violation` (`params[0] == "value"` on an ADEPT `<meta>`) | AutoSafe | Adobe DRM left `<meta name="Adept.resource" value="…">`, and `<meta>` has no `value` | [Rename `value` to `content`](#rsc-005--an-adobe-adept-meta-carrying-value) |
| `RSC-005` | `opf.content_document.schema_violation` (empty `lang`/`xml:lang`) | ConfirmNeeded | An empty language tag, which EPUB 2's grammar does not allow | [Delete the attribute](#rsc-005--an-empty-lang--xmllang) |
| `RSC-005` | `opf.content_document.schema_violation` (`params[0] == "id"`) | ConfirmNeeded | An `id` that is not a valid XML NCName (on the shelf: it starts with a digit) | [Rename it, moving every reference with it](#rsc-005--an-id-that-is-not-a-valid-ncname-the-first-cross-file-fixer) |
| `RSC-005` | `opf.package.schema_violation` | AutoSafe | An EPUB 3 attribute on an EPUB 2 package document | [Delete it, once verified it says nothing the book does not](#rsc-005--an-epub-3-attribute-on-an-epub-2-package-document) |
| `RSC-005` | `opf.content_document.duplicate_id` | ConfirmNeeded | Two or more elements in one document share an `id` | [Keep the first, rename the later ones](#rsc-005--a-duplicate-id-in-a-content-document) |
| `RSC-007` | `opf.content_document.reference_missing_resource` | ConfirmNeeded | A link's path is stale, but the file it names is still in the book | [Repoint the path, carry the fragment across](#rsc-007--a-reference-whose-path-is-wrong-but-whose-target-is-in-the-book) |
| `RSC-007` | `css.font_face.missing_target` | ConfirmNeeded | A `@font-face` sources a font file the book does not contain | [Drop the whole rule; decline one with a second source](#rsc-007--a-font-face-whose-font-file-is-not-in-the-book) |
| `RSC-007` | `opf.content_document.reference_missing_resource` (a `<script src>`) | ConfirmNeeded | A script element names a file the book does not contain | [Delete the element; drop `scripted` if nothing else needs it](#rsc-007--a-script-src-whose-file-is-not-in-the-book) |
| `RSC-005` | `opf.package.opf_identifier_not_empty` (an extra one) | ConfirmNeeded | An empty `<dc:identifier>` with no `id`, beside a real one | [Drop it; never the unique-identifier anchor](#rsc-005--an-empty-extra-dcidentifier) |
| `OPF-001` | `opf.package.unrecognized_version` | ConfirmNeeded | The package declares a `version` that is not an EPUB version, so the book has never been validated | [Set it to `2.0` when only EPUB 2 fits](#opf-001--a-package-version-that-is-not-an-epub-version) |
| `OPF-035` / `OPF-043` | *(none)* | ConfirmNeeded | An XHTML file is declared `text/html` (a warning in EPUB 2, an error in EPUB 3; either way the file is never fully checked) | [Declare it `application/xhtml+xml`, once the file is shown to be XHTML](#opf-035--opf-043--an-xhtml-file-declared-texthtml) |
| `OPF-029` | `opf.manifest_item.declared_media_type_mismatch` (both types core raster) | ConfirmNeeded | An image is declared as one core image type and holds another (a PNG declared `image/jpeg`) | [Declare the type the file holds](#opf-029--an-image-declared-as-another-core-image-type) |
| `OPF-030` / `RSC-005` | `opf.package.unique_identifier_unresolved`, `opf.package.opf_identifier_not_empty` | ConfirmNeeded | The package's declared unique identifier resolves to nothing usable | [Attach the declared id to the book's one real identifier](#opf-030--rsc-005--the-packages-declared-identifier-points-at-nothing-usable) |
| `RSC-005` | `htm.epub2_dom.nested_anchor` | ConfirmNeeded | An `<a>` with only an `id` wraps a real link | [Unwrap it, moving the `id` to the child](#rsc-005--an-anchor-target-wrapped-around-a-link) |
| `RSC-005` | `opf.content_document.no_interactive_in_a__a` | ConfirmNeeded | The same nesting, in an EPUB 3 document | [The same repair, under the same fix id](#rsc-005--an-anchor-target-wrapped-around-a-link) |
| `RSC-005` | `navdoc.ol.empty` | ConfirmNeeded | An optional `<nav>` in the navigation document holds an `<ol>` with no `<li>` | [Delete the whole `<nav>`](#rsc-005--an-optional-nav-whose-list-is-empty) |
| `OPF-054` | *(none)* | ConfirmNeeded | A `<dc:date>` holds no date at all (EPUB 2) | [Drop the empty element; never touch a non-empty one](#opf-054--an-empty-dcdate-epub-2) |
| `OPF-072` | `opf.metadata.empty_element` | ConfirmNeeded | An optional Dublin Core element is empty (EPUB 2) | [Drop it; never a required one](#opf-072--an-empty-optional-dublin-core-element-epub-2) |
| `RSC-005` | `opf.package.dc_value_not_empty` | ConfirmNeeded | A Dublin Core element other than the required three is empty (EPUB 3) | [Drop it, `dc:date` included; only what was reported](#rsc-005--an-empty-dublin-core-element-epub-3) |
| `RSC-005` / `RSC-025` | `svg.attributes.attribute_not_allowed` (a lowercased camelCase name) | AutoSafe | An SVG attribute was lowercased by an HTML tool (`viewbox`), so an XML reading system ignores it | [Restore the name's case from the HTML standard's own table](#rsc-005--rsc-025--an-svg-attribute-whose-name-lost-its-case) |
| `RSC-005` / `RSC-025` | `svg.elements.element_not_allowed` (a lowercased camelCase name) | AutoSafe | An SVG element was lowercased by an HTML tool (`lineargradient`), so an XML reading system does not know it | [Restore the name's case from the HTML standard's own table](#rsc-005--rsc-025--an-svg-element-whose-name-lost-its-case) |
| `RSC-005` | `opf.content_document.schema_violation` (`params[0] == "dir"`, an uppercase `LTR`/`RTL`) | AutoSafe | A `dir` value written in capitals, which the grammar does not accept | [Lowercase it to `ltr` / `rtl`](#rsc-005--a-dir-value-in-capitals) |
| `RSC-005` | `ncx.schema_violation` (`missing_attribute`, `params` = `[element, "id"]`) | ConfirmNeeded | An NCX `navPoint` has no `id`, which the NCX grammar requires | [Give it a new id, unique in the NCX](#rsc-005--an-ncx-navpoint-with-no-id) |
| `RSC-005` | `opf.content_document.schema_violation` (`element_not_allowed`, `params[0]` = `ul`/`ol`, inside a list) | ConfirmNeeded | A nested list sits directly in its parent list, after an `<li>`, instead of inside it | [Move it into the end of the `<li>` before it](#rsc-005--a-nested-list-sitting-directly-in-its-parent-list) |

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
entity denotes. Example: `&nbsp;` → U+00A0, `&mdash;` → `—`, `&emsp;` → U+2003.

The names are those of the three XHTML 1.x DTD entity sets (`HTMLlat1`,
`HTMLsymbol`, `HTMLspecial`; 248 names besides the XML-predefined five), read
from epubveri's `htm::xhtml_entity` — the table it declares from when it reads an
EPUB 2 document, so the detector and the repair cannot disagree about what a
name means. Each name denotes one code point, fixed by those DTDs. (Until 0.30.0
this was a curated 117-name subset of the same table, and a book using any of
the other 131 — `&emsp;` — was left unopenable.)

**Why it's safe.** These are standard HTML named entities; substituting the
character they denote changes only the *encoding* of that character, not the
rendered content. The result no longer relies on an undeclared entity, so the
document becomes well-formed and the finding clears.

**When it declines.** Any name **not** in those three sets is left untouched (and
stays reported) — HTML5's much larger list of named references (`&NewLine;`) is
not XHTML's, and a name outside the DTDs is never guessed. The XML-predefined
five (`&amp;` `&lt;` `&gt;` `&quot;` `&apos;`) are always declared and so never
appear here.

**Measured (562 books, 2026-10-16, the 0.30.0 change).** On the shelf as it is,
no output changes: only one book uses any of the 131 names, and its EPUB 2 DTD
declares them. After epublift's EPUB 2 → 3 upgrade (cli-v4.0.0), that book —
*On Küçük Zenci*, 12 × `&emsp;` in 4 documents — carried 12 fatal findings,
which this clears with no other count rising.

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

- **the character it denotes**, when the name is one we map (the same
  XHTML 1.x table as `html_entities`) — this both closes and resolves the reference, leaving
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

**Trimming, as epubcheck trims (2026-10-08).** Both sides are trimmed of XML
whitespace only — Java's `trim`, which keeps a NO-BREAK SPACE — and
`unique-identifier` is read as written against a trimmed `id`
(`idAttr.trim().equals(uniqueIdent)`). With Rust's `str::trim` a `dtb:uid` that
differs from the identifier only by a trailing no-break space looked already
correct and the real NCX-001 was declined, and an identifier ending in one was
written into the NCX without it, which epubveri still reports as a mismatch:
a repair that changes the file and clears nothing, and which rollback cannot
see. No book on the 544-book shelf has a padded identifier (probe verified on a
fixture that has one); the fix is argued and unit-tested.

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
   collapsed to one line;
3. failing both, the **book's title**: the first non-empty `<dc:title>` in the
   package metadata (EPUB 3.3: the first `dc:title` is the main title),
   whitespace collapsed. *Added 2026-10-14, see below.*

The text is XML-escaped and only the empty `<title>` element is rewritten.

**Why it's safe.** The title is never *invented*: every source is the book's own
words, authored by whoever made the book. The first two name the document; the
third names the work the document belongs to. The title element is
document metadata — it is not part of the rendered text — so filling it changes
nothing a reader sees in the content, and it clears a genuine content-model
violation. It is `ConfirmNeeded` rather than `AutoSafe` precisely because it adds
visible metadata: the user sees the exact text before approving it.

**When it declines.** When the book has **no** source: no TOC entry, no heading
and no non-empty `dc:title`. It also declines a document that won't parse, or
whose title turns out not to be empty after all (a stale finding never
overwrites real text).

**The `dc:title` fallback (2026-10-14), and the decision it reverses.** Until
0.27.0 the fixer declined when neither the TOC nor a heading named the document
(on the shelf: image-only covers, dividers, and calibre's split continuations).
We called the book's title "a guess about intent". JSWolf (MobileRead 374799
#53) answered that the book's title is what to use when nothing else names the
document, and that this is how EPUB 2 documents are commonly titled. On
reflection the fallback meets our rule. `<title>` is document metadata, not
rendered text. `dc:title` is the book's own words, not ours. And the user sees
the text before approving it (ConfirmNeeded). It stays last: the document's own
name, where the book gives one, is the better title, so the first two sources
keep their order.


---

## RSC-005 — a `<head>` with no `<title>`

> **Written spec-first on 2026-10-14.** The finding shape below is what
> epubveri 0.27.0 reported on the shelf before any code existed.

**Finding.** `opf.content_document.schema_violation`, kind `incomplete_content`,
`params` = `["head", "title"]`, `element_path` = `/h:html[1]/h:head[1]`. The
document's `<head>` has no `<title>` element at all, which both XHTML 1.1 and
HTML require. (An *empty* `<title>` is the previous section's finding.)

**The shape.** On the shelf, word-processor and calibre output, most of it
split continuations:

```html
<head>
<meta name="Generator" content="Atlantis Word Processor 1.6.5.2"/>
<meta content="http://www.w3.org/1999/xhtml; charset=utf-8" http-equiv="Content-Type"/>
…
</head>
```

**Fix** (`fix.missing_title`, ConfirmNeeded). Insert `<title>…</title>` as the
first thing inside `<head>`, right after its start tag. The text comes from the
same three sources as `fix.empty_title`, in the same order: the book's TOC label
for this document, else its first heading, else the book's `dc:title`. The text
is XML-escaped. Nothing else in the document changes. One proposal per document,
as for `fix.empty_title`.

**Why it is safe.** The same argument as `fix.empty_title`: the text is the
book's own, and `<title>` is metadata, not rendered text. Placing it first is
valid in both grammars: XHTML 1.1 lets `<title>` sit anywhere among the other
head elements, and HTML puts no order on them.

**When it declines.**

- **No source**: no TOC entry, no heading, no non-empty `dc:title`.
- **A finding other than exactly `["head", "title"]`**, or a `<head>` that
  already has a `<title>` (a stale finding).
- **A self-closing `<head/>`**, or a document that does not parse.

**Measured (562 books, epubveri 0.27.0, before building).** 8 books, 98
findings, all at `/h:html[1]/h:head[1]`. Seven are the Atlantis/calibre shape
above; one (40 findings) is an EPUB 2 package whose generated pages open with
`<head><style>`.

**EPUB 3 says it differently, and the fix answers both.** On EPUB 3 the same
absence is the *warning* `RSC-017` / `opf.content_document.head_missing_title`
(epubcheck's schematron), not a grammar error. Same element, same repair; the
proposal names whichever finding it addresses. On the shelf: 3 books, each an
InDesign `navigation.xhtml`.

**Measured after building (562 books, epubveri 0.27.0, 2026-10-14).**
`fix.missing_title` makes 101 proposals (98 EPUB 2, 3 EPUB 3) and
`fix.empty_title` 198, up from 191: the 7 it used to decline now take the
book's title. Fully valid after an approve-all repair: 103 → 110 books. No
regression at any severity.

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

**It repairs only the manifest.** The same file may be named again from the NCX,
and that reference is a separate finding with its own fixer — see below. A book
carrying both needs both, and clearing only one leaves it invalid.

---

## RSC-020 — unencoded space in an NCX `<content src>`

**Finding.** `opf.ncx.content_src_unencoded_space`. A `<navPoint>`'s
`<content src>` contains a raw space; epubveri reports the offending `src` in
`params[0]`, and the finding is located in the **NCX**, not the package document.

**Fix** (`fix.ncx_content_src_spaces`, AutoSafe). In that NCX, percent-encode
each space in every `<content>` whose `src` is the reported value. The quote
style and every other attribute are preserved.

**Why it's safe.** It is the same argument as its manifest sibling, and the same
edit: a `src` is a URL, a raw space is not a legal URL character, and `%20`
resolves back to exactly the same container entry. The file is not renamed.
epubveri measured this directly against epubcheck 5.3.0 on a book with one
content document named `a b.xhtml`: with both references raw the book is invalid
in both tools, with **only the manifest encoded it is still invalid**, and with
both encoded both tools call it valid.

**One `src` value, every element carrying it.** A single document is typically
named by many `<navPoint>`s — 28 of them in the worst book on the shelf — and
epubveri reports one finding per navPoint. All of them are the same edit, so the
fix is planned once per NCX and encodes every matching element.

**When it declines.** If the NCX won't parse, or no `<content>` carries the
reported `src` verbatim, no edit is made.

**`PKG-006` remains afterwards, and that is correct.** It is a *warning* about
the space in the ZIP entry name itself, and the book's verdict is valid with it
present. Renaming container entries is not something any fixer here does.

---

## RSC-020 — unencoded space in a content document's URL

> **Written spec-first on 2026-10-16**, before the code.

**Finding.** `RSC-020` / `opf.content_document.malformed_relative_url`,
**error**, both versions. A relative URL in a content document is not a valid
URL. `params[0]` is the attribute's value exactly as the file holds it, and
`element_path` names the element carrying it
(`/h:html[1]/h:body[1]/h:p[10]/h:img[1]`, or `…/svg:svg[1]/svg:image[1]` for a
cover's `xlink:href`). The attribute itself is not named. The rule covers five
defects (an unencoded space, a backslash, a `%` not followed by two hex digits,
a control character, a character URLs do not allow), and the message names the
first one only — so a URL with a space *and* a backslash is reported once.

On the shelf (562 books, 2026-10-16) every case is a space: 6 findings in
3 books (`<img src="../Images/Screen Shot 2018-01-07 at 23.03.10.png">`, two SVG
covers). After epublift's EPUB 2 → 3 upgrade (cli-v4.0.0) there are 135 in 7
books: the same 6, plus 129 in the `nav.xhtml` the upgrade writes from the NCX
(`<a href="GururVe Onyargi_split_004.html">`), and for 5 of the 7 books this is
the only error left.

**Fix** (`fix.content_url_spaces`, AutoSafe). On the reported element, in the
one attribute whose value is exactly `params[0]`, percent-encode each **interior**
space as `%20`. Leading and trailing spaces are left as they are: the URL
parser strips them, they are valid, and encoding one would make it part of the
path. Nothing else in the value is touched. One proposal per document.

**Why it's safe.** The same argument as the manifest and NCX siblings: a raw
space is not a legal URL character, `%20` is its one correct spelling, and it
resolves back to exactly the same container entry. No file is renamed. The
manifest and the NCX already name these files in their own, separately
repaired references; this is the third place a book spells the same path.

**Only where the detector then accepts the URL.** No table of URL defects is
kept here — that would be a second oracle. Every planned encoding is applied to
a copy of the book, and an encoding whose result epubveri still reports under
this rule (the URL had a second defect besides the space) is declined, since it
would change the file and fix nothing. The pattern is
[`fix.svg_attribute_case`](#rsc-005--rsc-025--an-svg-attribute-whose-name-lost-its-case)'s.

**When it declines.**

- **No attribute, or more than one, on the reported element carries the value
  exactly.** `<img alt="a b.png" src="a b.png">` is declined: which one is the
  URL is not ours to guess from the name. (On the shelf, `alt` drops the
  extension, so it never matches.)
- **The value in the file is not the value in `params[0]`**, byte for byte — an
  entity or character reference inside it (`a&#32;b.png`). Rewriting the
  attribute would then change how it is spelled, not only the space.
- **The trial still reports it** — a second defect after the space, or no
  interior space at all (the edit would then change nothing).
- A document that will not parse, or whose reported element is not found.

**Measured (562 books, 2026-10-16).** All 6 findings in the 3 books repaired,
no other count moved, and one book (an SVG cover) now validates. On the
upgraded copies, all 135 in 7 books repaired, no count rose, none reverted,
and 5 books now validate. No case on the shelf exercises a decline: every
reported URL's only defect was the space, and no element carried the value
twice.
 — undeclared content property

**Finding.** `opf.content_document.property_used_undeclared`. A content document
uses a feature — `scripted`, `svg`, `remote-resources`, or `switch` — that its
manifest `<item>` does not declare. epubveri names the property in `params[0]`.

**Fix** (`fix.content_properties`, AutoSafe). Add the token to the `properties`
attribute of the manifest item whose `href` resolves to that document (existing
tokens are kept; the attribute is created if absent). The item's href is resolved
the way the validator resolves it (`epubveri::opf::resolve`) — relative to the
OPF's directory, split on `/` and then percent-decoded per segment, with
`.`/`..` normalized — and compared with the document's entry name under NFC,
as the validator compares them (2026-10-08; the same holds wherever a fixer
asks which entry a reference names).

**Why it's safe.** epubveri has already *proven* the usage by finding it in the
document, so the declaration is not a guess — it is the manifest being made to
tell the truth about a document that is not itself modified. EPUB 3.3 requires
exactly this declaration.

**When it declines.** If the OPF won't parse, no manifest item resolves to the
document, or the property is already declared, nothing is changed.

**It also declines on the whole book when the package declares EPUB 2**
(`version="2.0"`, or the `1.x` OEBPS spellings). `properties` is an EPUB 3
attribute and OPS 2.0.1's manifest has no equivalent, so adding one clears the
reported defect and produces `attribute "properties" is not allowed here` in its
place — an error the book did not have. There is no edit that repairs such a
book; declining is the repair. A package whose `version` cannot be read is not
treated as EPUB 2, since declining there would withdraw a repair on a guess.

---

## PKG-006 / PKG-005 — the `mimetype` entry's packaging

**Finding.** `PKG-006` (no `rule` sub-code — the code is unambiguous on its own,
and its subject is the container itself, so there is nothing to disambiguate).
The archive has a `mimetype` entry, but it is not the first one. OCF requires the
`mimetype` entry to come first and to be stored uncompressed, so that a reader
can identify the file by reading its opening bytes.

`PKG-005` (also no `rule`): the `mimetype` entry is first, but its ZIP header
carries an extra field, so its contents do not sit at the fixed offset a reader
looks at. On the test shelf it is a 36-byte field in eight books.

**Fix** (`fix.mimetype_packaging`, AutoSafe). Re-emit the `mimetype` entry first
and stored, with a header the writer derives, which carries no extra field. One
edit repairs both findings; when both are present the proposal names `PKG-006`.
Every other entry keeps its original order, bytes and compression.

**`PKG-005` was invisible until 0.22.0, and that was a bug, not a decline.** The
writer cannot carry local-header extra fields, and epubsana's "before" report
used to validate the writer's output rather than the book itself. So `PKG-005`
never appeared, and three shelf books whose only error it was were reported
valid, with exit status 0, while the file on disk was not. The before report now
validates the original bytes. Found by epublift, comparing epubsana's verdicts
with epubveri's on the original files.

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

## PKG-014 — an empty directory entry

**Finding.** `PKG-014` (a warning, no `rule`). The ZIP holds a directory entry,
such as `OEBPS/fonts/`, with nothing inside it: a tool moved or removed the files
and left the folder behind. The finding's location is the directory's name.

**Fix** (`fix.empty_directory`, AutoSafe). Leave the directory entry out of the
written archive. One proposal per book, naming every directory it drops.

**Why it's safe.** A directory entry carries no content, and no reading system
reads one. Every file in the book keeps its bytes, name and order. The emptiness
is checked here against the archive, not taken from the finding.

**When it declines.**

- A reported name that is not a directory entry, or that has anything inside it.
- **The whole book**, if dropping the reported directories would leave another
  directory entry empty (`OEBPS/` holding only `OEBPS/fonts/`). That would trade
  one `PKG-014` for a new one the detector never reported.

**Measured.** 544 books, epubveri 0.20.0: three books, two directories each,
all six cleared, no new finding at any severity. Found by epublift: those three
books' only other finding was `PKG-005`, so epubsana used to leave them alone
entirely.

---

## RSC-005 — an empty extra `<dc:identifier>`

**Finding.** `opf.package.opf_identifier_not_empty`: a `<dc:identifier>` is
empty. The finding carries no path and no `params`. It is a Schematron
assertion, located at the package document only.

**Fix** (`fix.empty_extra_identifier`, ConfirmNeeded). Drop every empty
`<dc:identifier>` that has no `id`, provided a non-empty `<dc:identifier>`
remains in the book.

**Why it's safe.** An identifier with no `id` cannot be referred to, so nothing
points at it. With a real identifier left, the requirement for one still holds.
The rule that keeps `dc:identifier` out of `fix.empty_metadata_element` is "do
not trade *empty* for *missing*", and it does not reach an extra one.

**When it declines.**

- **It never touches the empty identifier that the package's
  `unique-identifier` names.** Filling it, or moving its `id` to one of the real
  identifiers, is choosing the book's identity. That is
  `fix.package_identifier`'s shape, which declines when more than one candidate
  exists.
- No non-empty identifier in the book: deleting one would leave none.
- **The count does not match.** Every empty `<dc:identifier>` draws one
  finding, so the number of findings in the package must equal the number of
  empty elements this fixer sees (empty means epubveri's own
  `xmlext::is_xml_blank`). If they differ, the detector and the fixer disagree
  about what is empty, and nothing is touched.

**Measured.** One book on the shelf, from Calibre: an empty anchor, an empty
extra and two real identifiers. Only the extra goes. Found by epublift.

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
- **If more than one manifest item carries it.** An `id` is meant to be unique,
  and a book that breaks that is reporting a second defect rather than granting a
  guess. `compute_dangling_item_edits` used to take the first match, and on the
  474-book shelf one book had two `<item id="added2">` — one naming the missing
  font the finding is about, one naming a font the container really holds. It
  deleted the *present* one: `RSC-001` stayed, an `OPF-003` was authored, and a
  real resource lost its only declaration. Narrowing by the reported href instead
  was rejected — the manifest's spelling of an href and epubveri's need not agree
  character for character, so it would trade a wrong deletion for a
  sometimes-wrong one — and the ambiguity is not confined to the item lookup
  anyway, since `<itemref idref>` and `<meta name="cover" content>` key on the
  same id.
- **If `<spine toc="…">` or `<spine page-map="…">` names it.** Both attributes
  hold a manifest id and neither is an `<itemref>`, so the spine walk every other
  check here uses cannot see them. On the same shelf, dropping the item named by
  `<spine page-map="_page_map_">` cleared `RSC-001` and authored `OPF-063` in its
  place — the nav guard's trade at a third site. Repairing it properly would mean
  deleting the attribute too, which is a different edit on a different element and
  is not this fixer's to make. `toc` is guarded on the argument that it is the
  same shape, not on a measurement: no shelf book has a dangling NCX item.
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
- **If anything else in the container still links to the missing file.** The
  absent `cover.jpg` is reported once, against the manifest (`RSC-001`); drop the
  declaration and the same absent file is reported again, against the reference
  that named it (`RSC-007`, `reference_missing_resource`). Deleting a declaration
  does not make a file present. This is the nav guard's principle a second time:
  a repair that trades one finding for another is not a repair.

  The package document is excluded from the scan, because its own manifest entry
  is exactly what is being dropped and the `<meta name="cover">` that may name it
  goes in the same edit. Everything else that can hold a reference is scanned as
  **text rather than parsed** — these are books being repaired *because* their
  markup is defective, and a document that failed to parse would silently answer
  "nothing references it", which is the unsafe direction for a guard. A candidate
  counts only when it is a whole quoted attribute value that resolves, against
  the referring document's own directory, to the missing entry: the filename in
  prose, in a comment, inside a longer string, or in a remote URL is not a
  reference.

  **An escaped link is a link (2026-10-08).** The scan searches for the
  filename as is and with its space and non-ASCII bytes percent-escaped, and
  resolves every hit with epubveri's `opf::resolve`, which decodes. Before
  this, a manifest `href` containing `%` skipped the guard altogether, and a
  link spelling `my%20pic.jpg` was never found — the guard failed open, leaving
  only rollback to catch the trade. A spelling the search does not try (mixed
  hex case in one name) can only make the guard see less, never invent a
  reference. The shelf has 17 escaped manifest hrefs in 9 books, none of them
  dangling.

**Measured.** 2 books in the 171-book corpus, both the same shape: a conversion
left `cover-1.jpg`/`cover-2.jpg` declared beside the real, present cover
(`id="cover"` → `cover.jpeg`, which is what `<meta name="cover">` actually names).
On this corpus neither guard fires — the dangling items are images, so nothing in
the spine references them, and they are not the declared cover. Grepping every
content document, the NCX and the OPF confirms the manifest entry itself is the
**only** thing in either book that mentions them.

**The reference guard is measured too, and it closes a check an earlier session
had been doing by hand.** The paragraph above ends by saying that grepping every
content document, the NCX and the OPF confirmed the manifest entry was the only
mention of those two items — the right check, run manually, on a 171-book corpus
where it happened to pass. On the 444-book shelf (2026-09-02) it does not: one
book declares a missing `cover.jpg` that its own `cover.xhtml` links to, and
dropping the item moved the finding from `RSC-001` to `RSC-007` while leaving the
book's error count **6 before and 6 after**. An instrument watching totals would
have called that a repair; `regression_audit`, which watches for findings that did
not exist before, called it the shelf's only regression. The guard costs exactly
one of the fixer's seven shelf proposals — the whole-shelf plan digest loses that
single line and nothing else — and takes the run back to introducing nothing.

**The two guards above were measured the same way, on 2026-09-09, and the first
of them is the sharpest example this file has of an error total lying.** Before
the guard, "repairing" *Kutadgu Bilig* deleted one of the two `added2` items —
which cleared **both** `duplicate id "added2"` errors as a side effect, so the
book's error count went *down* while a font it actually contains lost its
manifest entry and the reported `RSC-001` never moved. Adding the two guards
costs three errors that now stay unrepaired and exactly **two of the fixer's
eight shelf proposals** — the whole-shelf plan digest loses those two lines and
nothing else — and takes `regression_audit` back to introducing nothing at any
severity on 474 books. That is the fifth defect the shelf has found that no unit
test could, and the second and third in this fixer.

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

## RSC-005 — an Adobe ADEPT `<meta>` carrying `value`

**Finding.** `opf.content_document.schema_violation`, kind
`attribute_not_allowed`, `params[0] == "value"`, with an `element_path` ending
in `/h:head[1]/h:meta[k]/@value`. On real books it is always the same element,
written into every chapter by Adobe's DRM tooling:

```html
<meta name="Adept.resource" value="urn:uuid:…"/>
```

`<meta>` has no `value` attribute in XHTML 1.1 or in HTML. A named `<meta>`
carries its value in `content`.

**Fix** (`fix.adept_meta_value`, AutoSafe). Rename the attribute: `value="…"`
becomes `content="…"`. The name and the identifier are kept byte for byte, and
nothing else in the document changes. Every affected file in the book goes into
one proposal, because the decision is the same in each.

**Why it's safe.** Only the five bytes of the attribute's name change. Deleting
the element would also clear the finding, but it would throw the identifier
away; the rename keeps everything the book says. The fixer acts only on the
elements epubveri reported: each candidate's own path must be one a finding
names, so the fixer cannot act where the detector found nothing.

**When it declines.**

- A `<meta>` with any other `name`. Its `value` belongs to someone else's
  vocabulary, and nothing here says what it should become.
- A `<meta>` that already has `content`. There are then two values, and which
  one is right is a choice.
- A `<meta>` whose path no finding names, and a document that does not parse.

**Measured.** 544 books, epubveri 0.19.1: 445 findings in 11 books, one
proposal per book, all 445 cleared, and no new finding at any severity. The
rename and the deletion were both tried, and both cleared every finding; only
the rename keeps the identifier.

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

**Fix** (`fix.empty_lang`, ConfirmNeeded). **Two behaviours, decided by which
element carries the attribute.**

- **On the root `<html>`:** fill it with the book's own `<dc:language>` —
  `lang=""` → `lang="tr"` — when the package declares exactly one, non-empty and
  well-formed. The original spelling is kept, so an `xml:lang` stays `xml:lang`.
- **Anywhere else:** delete the attribute, with the whitespace that separated it.

One proposal per document; `lang` and `xml:lang` on the same element go together
in it, since a document that has one almost always has the other.

**The split was Doitsu's suggestion (MobileRead, 2026-08-13) and the corpus
settled it.** He proposed filling from `dc:language` instead of deleting. Probed
before answering: **all 447 empty `lang`/`xml:lang` attributes on the 157-book
shelf sit on `<html>`** — not one on an inline element — and all three affected
books declare a valid `dc:language` (`tr`, `en`, `tr`).

**That measurement is what makes filling correct, and it is worth stating why the
old reasoning failed.** This section used to argue: *`<p lang="">` inside
`<html lang="tr">` declares "undetermined"; delete it and the paragraph inherits
`tr`, which a reading system acts on.* Sound — **about a shape that does not
occur.** On the *root* element the situation inverts: there is no ancestor to
inherit from, so deleting leaves the document declaring no language at all, while
filling gives it the language the book itself states. Nothing is invented; the
value is read out of the book, exactly as `fix.empty_title` reads a title out of
the book's own table of contents. The two fixers now apply one principle instead
of two.

**Why it stays `ConfirmNeeded`.** Filling a root language tag tells a reading
system which hyphenation rules, text-to-speech voice and CJK font to use for the
whole document. That is right when `dc:language` is right, and `dc:language` is
metadata the same producer wrote — which is a reason to ask rather than assume.
Deletion, where it still applies, is not inert either, for the original reason:
an element that declared "undetermined" starts inheriting its parent's language.

**Declines, in the filling branch.** Any of these falls back to deleting:

- the package declares **more than one** `<dc:language>` — which is the document's
  root language is then editorial;
- it declares none, or an empty one;
- the value is not a well-formed language tag (`en_US`, `turkish`), since writing
  it would trade an invalid empty tag for an invalid non-empty one.

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
  except here it cuts the other way. An EPUB 3 book with a *malformed* date is
  not invalid, so there is nothing for a `--goal valid` repairer to clear. An
  EPUB 3 book with an *empty* one is: since epubveri 0.13.3 it also draws
  `opf.package.dc_value_not_empty`, and [that entry](#rsc-005--an-empty-dublin-core-element-epub-3)
  drops it.
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

## OPF-072 — an empty optional Dublin Core element (EPUB 2)

**Finding.** `OPF-072` / `opf.metadata.empty_element`, reported on the package
document: *dc:X metadata element is empty*, with `params[0]` = the element name
(`"dc:coverage"`, `"dc:source"`, …). The sibling of the empty-`<dc:date>` fixer
above, generalised to the rest of the optional Dublin Core set.

**Read the emission site first — it has already done most of this fixer's safety
work** (`epubveri/src/opf.rs:3179`):

- **It is EPUB 2 only** (`if is_epub2`), the same version-scoped split as
  `OPF-054`.
- **It excludes `identifier`, `date`, `title` and `language` by name.** The three
  required elements can therefore never reach this fixer, so the "would deleting
  this create a *missing* element?" question cannot arise. `date` is excluded
  because it has its own rule and its own fixer — **there is no overlap with
  `fix.empty_dc_date`**, and a book cannot draw two proposals for one element.
- The exclusion list carries upstream's own comment explaining that an empty
  `dc:identifier` used to draw a spurious `OPF-072` and an empty `dc:language`
  drew one where epubcheck gives `OPF-055`. So the list is a deliberate
  correctness boundary, not an accident of implementation.

**Fix** (`fix.empty_metadata_element`, ConfirmNeeded). Delete every empty
optional Dublin Core element in `<metadata>`, together with the whitespace that
preceded it. One proposal per package document.

    <dc:source/>
    <dc:coverage></dc:coverage>
    →
    (both removed)

**Why it is safe.** An empty element states nothing — there is no value in it to
lose — and every element that can reach this rule is optional in EPUB 2, so its
absence is valid. This is the identical argument the `<dc:date>` fixer makes, and
it is the whole of the justification: the repair removes a declaration that
declares nothing.

**Two guards, both vacuous on the corpus and both kept anyway.**

- **The required-element check.** `dc:title`, `dc:identifier` and `dc:language`
  are never deleted, even though the detector already excludes them. A fixer that
  silently depends on an upstream exclusion list breaks quietly the day that list
  moves, and this one deletes things. The same invariant `fix.empty_dc_date`
  carries, and vacuous for the same reason.
- **The refinement check.** An element whose `id` is the target of a
  `<meta refines="#id">` is declined rather than deleted, since removing it would
  orphan the refinement — trading this finding for another. Taken verbatim from
  `fix.empty_dc_date` rather than reinvented: the first draft of this spec said
  "decline any element carrying an `id`", which is stricter, no safer, and would
  have left two sibling fixers disagreeing about the same class for no reason.
  `refines` is an EPUB 3 mechanism and this rule is EPUB 2 only, so it is vacuous
  here in any case.

**A third guard, added 2026-09-09: an element is empty only when its content is
XML whitespace.** Rust's `str::trim` strips the Unicode `White_Space` class, so
an element whose whole content is a NO-BREAK SPACE looked empty and was deleted.
epubcheck 5.3.0 accepts it, and epubveri stopped reporting it at 0.13.7 — one of
four sites it fixed there, two of them verdict-moving.

**Upstream's fix does not cover this fixer, and the mechanism is worth
understanding because it applies to every file-dispatched fixer here.** This one
takes only the *file* from the finding and then re-derives every droppable
element itself. So when an NBSP-only `<dc:description>` sits beside a genuinely
empty `<dc:source>`, epubveri reports the second alone and epubsana dropped both:
the deletion never needed a finding of its own. Measured as a fixture
(`nbsp_probe.rs`), because no book on the 474-book shelf holds the shape.

The predicate is `epubveri::xmlext::is_xml_blank` rather than a local copy, on
purpose. Upstream's own account of how the bug survived is that `is_xml_space`
already existed and already documented the trap, while four sites kept reading
`trim()` — a migration written up and left half done. One spelling in the family
is the guard against a second half-migration. `fix.empty_dc_date`,
`fix.empty_title` and the nested-anchor unwrap read the same predicate; the last
of those on a different argument, that an NBSP in a content document is a space a
reader can see.

**Measured on the 157-book shelf (2026-08-13): 29 findings across 9 books.**
Every one is `dc:coverage`, `dc:relation`, `dc:rights`, `dc:source`,
`dc:subject` or `dc:description`. **27 are self-closing (`<dc:source/>`) and 2
are empty pairs (`<dc:coverage></dc:coverage>`)** — both shapes are handled, and
the probe found no third shape. **None carries an `id`, and none is a required
element**, so both guards decline nothing on this corpus.

**It does not move `--goal valid`.** `OPF-072` is *usage* severity: these books
are not invalid because of it and clearing all 29 changes no verdict. The fixer
exists because a publisher reading an epubcheck report has 29 lines of noise in
it that describe nothing, and removing noise that describes nothing is exactly
the burden this tool is for. Do not quote it as coverage of anything.

## RSC-005 — an empty Dublin Core element (EPUB 3)

**Finding.** `RSC-005` / `opf.package.dc_value_not_empty`, **error**, EPUB 3
only, reported on the package document at the element: *dc:X must be a string
with length at least 1*, with `params[0]` = the qualified name (`"dc:rights"`).
The EPUB 3 half of [`OPF-072`](#opf-072--an-empty-optional-dublin-core-element-epub-2):
the same defect, but here it is a validity failure. EPUB 3.3 §5.5.2: DCMES
elements *"MUST have non-empty values after leading and trailing ASCII
whitespace"*.

**Read the emission site** (`epubveri/src/opf.rs`, `check_dc_values_not_empty`):

- **It excludes `identifier`, `title` and `language` by name** — each has a rule
  of its own — and **includes `date`**, which keeps its `OPF-053` warning as well
  (epubcheck reports both on an empty date).
- **Empty means: the text of all descendants is XML whitespace**, the predicate
  `compute_empty_metadata_edits` already reads. The EPUB 2 site reads direct
  text children instead (the Calibre `<p>` case). So pointing this fixer at the
  EPUB 3 rule does not inherit the wrong test; that worry in `docs/COVERAGE.md`
  was written before our predicate was pinned to descendants.
- It walks every `dc:*` in the document; this fixer walks only `<metadata>`'s
  children. That makes our candidates a subset, never a superset.

**Fix** (`fix.empty_metadata_element`, ConfirmNeeded — **the same fixer and the
same id**). Delete every empty Dublin Core element in `<metadata>` except
`dc:title`, `dc:identifier` and `dc:language`, with the whitespace that preceded
it. One proposal per package document. The fixer reads which of the two rules
was reported for the file and takes that rule's set: on EPUB 2 `dc:date` is still
left to `fix.empty_dc_date` (`OPF-054` fires there, so two fixers would edit one
range); on EPUB 3 `OPF-054` never fires, and nothing else drops an empty date.

**Why one id.** The repair is identical and so is its argument; only the
detector's version split differs. epublift already approves
`fix.empty_metadata_element` by id after its EPUB 2 → 3 upgrade, and that
approval silently did nothing on the result. With one id it starts working with
no change on their side, and the rule for "what is empty" lives in one place.

**Why it is safe.** An empty element states nothing, so there is no value in it
to lose, and every element this can reach is OPTIONAL in EPUB 3.3 (§5.5.3.2.1:
all but `dc:identifier`, `dc:language` and `dc:title`). For `dc:date`
specifically (§5.5.3.2.4): it is OPTIONAL, at most one is allowed, and it is the
publication date, not `dcterms:modified` — so dropping an empty one asserts
nothing about either date, and filling it would be inventing one.

**Guards.**

- **The required three** are never deleted, as on EPUB 2.
- **The refinement check**, taken over unchanged. It is vacuous on EPUB 2;
  here it is live, since `<meta refines>` is the EPUB 3 mechanism (epublift's
  upgrade writes it for `opf:role` / `opf:file-as`). An empty `<dc:creator
  id="c1">` with a `role` refining it is left alone, finding and all: dropping
  only the creator orphans the `meta`, and dropping both deletes a statement.
- **Every element we would drop must have been reported.** The findings carry
  `params[0]`, so the fixer counts reported names per file and declines the
  whole proposal if it would drop more of any name than epubveri reported. The
  two predicates agree today, so this cannot fire on a real report; it fires
  the day either side moves, which is the failure `fix.empty_metadata_element`
  already had once (the NBSP case above: a file-dispatched fixer deleted what
  was never reported). Tested with a report that names fewer elements than
  the file holds — the same count-guard shape as `fix.empty_extra_identifier`.
  The EPUB 2 path does not get it: there our descendants test is the narrower
  one, so our candidates are already a subset by construction.

**Measured (2026-10-16), and the measurement is not of the shelf as it stands.**
The shelf's inputs carry 0 findings of this rule (400 of 474 were EPUB 2 when
`docs/COVERAGE.md` counted). After epublift's upgrade (local build
`3.3.0+89d81e3.dirty`) of all 562 books, epubveri 0.28.0 reports **59 books /
212 findings**: `dc:source`, `dc:relation`, `dc:coverage`, `dc:rights` 41 each,
`dc:subject`, `dc:description` 21 each, `dc:publisher` 5, `dc:creator` 1.
`-y` on those 59, 0.28.0 against this fixer, per book: **212 → 0, no `(id,
rule)` count rose, nothing reverted.** No refinement and no count guard declined
anything there; both are exercised by fixtures only. **An empty `dc:date` is not
among the 212**: that case is verified on fixtures only.

## OPF-090 — a manifest item declaring a non-preferred Core Media Type

**Finding.** `OPF-090` / `opf.manifest_item.non_preferred_media_type`, **Usage**,
EPUB 3 only, reported on the package document: *media-type 'X' is a non-preferred
(but valid) Core Media Type*, with `params[0]` = the declared type **exactly as
written** (any `; charset=…` parameter included) and `params[1]` = the type that
supersedes it. The resource is fine and the book is valid; the declaration uses a
superseded name for a format that has a current one.

**`params[1]` is optional and its absence is the detector's answer**, not a gap.
epubveri names no replacement when the resource's own signature rules the
row's candidate out — an `OTTO` (CFF) font declared `application/x-font-ttf`, a
non-WOFF declared `application/font-woff` — and when the spec's table names two
rows for one spelling and nothing decides between them. Read `params.get(1)`,
**never `params[1]`**.

**Fix** (`fix.non_preferred_media_type`, ConfirmNeeded). Rewrite the
`media-type` attribute to the type the finding names. One proposal per package
document.

    <item ... media-type="application/vnd.ms-opentype"/>
    →
    <item ... media-type="font/otf"/>

**Why it is safe.** Both names denote the *same* format. The edit renames a
declaration; it asserts nothing new about the bytes on disk, which is what makes
it different from `opf.manifest_item.declared_media_type_mismatch` (where the
declaration and the file genuinely disagree, and choosing between them is not
ours).

**The mapping is the detector's, not ours — changed 2026-09-11, and this is the
substance of the fixer.** It used to be a five-row table in `fixers.rs`, written
when epubveri held only a *set* of non-preferred types. epubveri has named the
replacement in `params[1]` since **0.12.3**, so the table had been a second
answer to a question the detector was already answering. Reading `params`
instead:

- **closes a gap the table could not see.** A file-dispatched fixer reading its
  own table renames every item whose declared type it recognises, including one
  the detector deliberately refused to name a target for. Zero books on the
  474-book shelf carry that shape (121 `vnd.ms-opentype` + 285
  `x-font-ttf` declarations, **none** ruled out by its own signature, 26
  unreadable) — so this is a closed hole, not a repaired one;
- **takes `application/font-sfnt` for free**, without this crate reading a
  single byte (see below);
- **keeps the detector boundary where it belongs**: deciding *which format a file is*
  is the detector's job, and this fixer now has no opinion of its own to drift.

**`application/font-sfnt` is repaired where the detector decides it, and
declined where it does not.** SFNT is the *container* TrueType and OpenType
share, and EPUB 3.3 lists the spelling on **both** rows — so the table names no
answer and this fixer used to refuse the whole case. epubveri 0.14.1 answers it
on its own side of the line: only `font/otf` admits `OTTO`, so a CFF-flavoured
sfnt resolves to `font/otf` whatever its filename says; a `glyf` sfnt is admitted
by both rows and falls back to epubcheck's guess from the extension; and a WOFF,
WOFF2 or collection gets no answer at all. Whatever arrives in `params[1]` is a
media type this fixer writes verbatim. **What this crate must never do is read
the signature itself** — that decides what the defect is.

| the detector says | this fixer does |
| --- | --- |
| `params[1]` present | rename to it |
| `params[1]` absent | **decline** — the detector refused to name a target |
| two findings, same declared type, different targets | **decline both** |

**The conflict rule is not hypothetical, and it is new with 0.14.1.** Because
the answer for `application/font-sfnt` now depends on each file's own bytes, one
package document can legitimately carry `application/font-sfnt → font/otf` for
one item and `→ font/ttf` for the next. The fix is per *declared type* across the
manifest and cannot honour both, so it proposes neither. Declining a repair that
exists is the cheap error here; renaming a font to the wrong format is the
expensive one, and it is the error JSWolf's report was mistaken for.

**Media-type parameters are matched past and dropped with the old name.** A
declaration reading `text/javascript; charset=utf-8` is matched on its base type,
the way epubveri matches it (`base_media_type`), and rewritten whole — the
parameter belonged to the name being replaced.

**It only ever acts where a finding is.** The proposal exists only for a package
document epubveri reported on, and only for declared types it reported there; an
EPUB 2 book draws no OPF-090 at all (the rule is EPUB 3 only) and is never
touched. `application/x-font-truetype` — 165 declarations on this shelf, the
second commonest font spelling here — is in **no** Core Media Type table, draws
no finding, and is correctly none of this fixer's business.

**Measured on the 474-book shelf (2026-09-11): 50 findings across 10 books**,
`application/vnd.ms-opentype` and `text/javascript` dominating.
**`application/font-sfnt` does not appear at all**, so taking the case costs and
buys nothing here; it is written for the book that has not arrived yet.

**It moves no verdict.** `OPF-090` is Usage severity: nothing becomes valid. It
removes a line from an epubcheck report that describes a name, not a fault.

## OPF-035 / OPF-043 — an XHTML file declared `text/html`

> **Written spec-first on 2026-10-16**, on epublift's report of the EPUB 3
> shape. The finding shapes below are what epubveri 0.28.0 reports.

**Findings.** Neither carries a `rule` or `params`; the finding's `position`
(line and column in the package document) names the element.

- **`OPF-035`**, warning, **EPUB 2 only**: *manifest item 'c1.html' should be
  declared application/xhtml+xml, not text/html*. Reported at the manifest
  `<item>`, whatever the file holds.
- **`OPF-043`**, error, **EPUB 3**: *spine item idref 'c1' has non-content
  media-type 'text/html' with no fallback*. Reported at the spine `<itemref>`.
  EPUB 2 exempts `text/html` here; EPUB 3 does not. The same finding is raised
  for any non-content type in the spine (audio, an image); only an itemref whose
  item is declared `text/html` is this fixer's.

**Fix** (`fix.html_media_type`, ConfirmNeeded). Rewrite that item's
`media-type` to `application/xhtml+xml`. Nothing else changes, in the package
or in the file. One proposal per package document.

    <item id="c1" href="c1.html" media-type="text/html"/>
    →
    <item id="c1" href="c1.html" media-type="application/xhtml+xml"/>

**Why it is one repair, not a choice.** The declaration and the file disagree,
which elsewhere (`declared_media_type_mismatch`) is a choice this crate does not
make. Here the other side of the choice does not exist: `text/html` is not an
EPUB content document type in either version, so no edit to the file could make
the declaration right. Only the declaration can move, and it can move to one
value. The fixer first shows that the file *is* XHTML: it parses as XML (after
the same DTD-entity step every structural fixer uses) and its root element is
`html` in the XHTML namespace. A file that is not XML is real HTML; relabelling
it would trade a warning for a fatal, and converting it is an editor's job.

**What happens next, and why it is not a regression.** epubveri does not check
a file declared `text/html` as a content document, in either version (epubveri
0.28.0, 2026-10-16):

- **EPUB 3: the file is not opened.** Like epubcheck, epubveri does not open a
  spine item whose type is not a content document type. Probe: a file with two
  broken references reports only `OPF-043` while declared `text/html`, and both
  `RSC-007` once declared `application/xhtml+xml`.
- **EPUB 2: the references are checked, the ids are not.** The same probe as
  EPUB 2 reports both `RSC-007` under either type. But on *Karanlık Orman*
  (91 such files), declaring them `application/xhtml+xml` showed 2
  `duplicate_id` errors that were always there (two elements with
  `id="bookmark0"`), and turned one NCX fragment from "may not be defined"
  (usage) into "is not defined" (error), because the detector could now see the
  file's ids. A first draft of this spec said EPUB 2 hides nothing; that was
  read from the probe alone, and the book refuted it.

So both ids join `opf.package.unrecognized_version` among the findings that
mean *the detector could not look*: a rise under a fix that lowered their count
is accepted as revealed, and a later fix that raises the same finding further is
still undone. Neither id has a `rule`, so both are matched by id. The price is
the one `fix.package_version` pays: any fix that lowers either count has its
rises accepted, and this is the only fixer that does. It rewrites attribute
values and nothing else.

**When it declines.**

- The node at the reported position is not the element the finding describes:
  not an `<item>` declared `text/html` (`OPF-035`), or not an `<itemref>`
  (`OPF-043`). A finding is never re-anchored to a guess.
- The itemref's `idref` names no manifest item, or two (a defective book breaks
  the uniqueness the lookup assumes), or an item not declared `text/html`
  (audio, an image). The same two checks hold for an `OPF-035` item, and are
  made again when the fix is applied. A `fallback` is not a reason: EPUB 3
  raises no `OPF-043` for an item that has one, and in EPUB 2 the file is
  XHTML whatever falls back to it.
- The item's file is missing, does not parse as XML, or its root element is not
  XHTML `html`.
- The package document does not parse.

**Tier.** ConfirmNeeded, like `fix.package_version`. The edit is small and
determinate, but it lets the detector check documents it has never fully
checked, and the book's count can rise. The title says so.

**Measured (562 books, epubveri 0.28.0, 2026-10-16, before building).** Shelf
inputs: 126 `OPF-035` in 5 books. The same 5 books after epublift's upgrade
(`4.0.0+64375f5`) and an epubsana repair: 126 `OPF-043`, every one on a
`text/html` item. One book holds 91 of them.

## OPF-029 — an image declared as another core image type

**Finding.** `OPF-029` / `opf.manifest_item.declared_media_type_mismatch`,
error, located at the **image's container path** (not the package document),
`params` = `[declared, actual]`: the type the manifest declares, and the type
epubveri sniffed from the file's signature (`image/jpeg`, `image/png`,
`image/gif`, `image/webp`, `image/tiff`, `image/bmp`). It is raised only when
the two differ; a file whose signature matches nothing draws the same id with
no rule and no `params`, and is not this fixer's.

**Why it is one repair here, when the rule in general is not ours.** The
declaration and the file disagree, and elsewhere choosing between them is not
this crate's (see OPF-090). For one shape the choice is not open: when the file
holds a **core image type**, the bytes are what every reading system shows
(readers sniff images), and the only content-preserving edit is to the
declaration. The other side would be re-encoding the image, which changes its
pixels or at least its bytes, and is image work. The `text/html` fixer rests on
the same argument: only the declaration can move, and to one value, the one
the detector found.

**Fix** (`fix.image_media_type`, ConfirmNeeded). In the package document,
rewrite the `media-type` of the one manifest item whose `href` resolves to the
reported path to `params[1]`. Nothing else changes: the file name keeps its
extension (`kolektif.jpg` holding a PNG still draws the `PKG-022` warning,
which is a separate, warning-level finding). One proposal per package
document, covering every such image. ConfirmNeeded, like the other
declaration fixers: the edit is determinate, but it rewrites what a publisher
wrote.

**When it declines.**

- **The file holds a type that is not core in both EPUB versions**: anything
  but `image/gif`, `image/jpeg`, `image/png`. A TIFF or BMP behind a core
  declaration needs converting, which is not a repair. WebP is core in EPUB
  3.3 but not in EPUB 2, so a WebP file declines too.
- **The declared type is not a core raster type** (`image/gif`, `image/jpeg`,
  `image/png`, `image/webp`). A foreign declaration may carry a fallback chain
  that was written for it; that is outside the narrow shape this admits.
- **The finding has fewer than two `params`.**
- **No manifest item, or more than one, resolves to the reported path** (a
  defective book breaks the uniqueness the lookup assumes), or the item's
  declared type (parameters stripped) is not `params[0]` (a stale finding).
- The package document does not parse.

**Measured (562 books, epubveri 0.31.0, 2026-10-22, before building; from
epublift's note, and `docs/COVERAGE.md` agrees on the count).** 4 books carry
the rule. One (a PNG declared `image/jpeg`) is admitted. Three hold a TIFF
behind a PNG or JPEG declaration and decline.

**Measured after building (562 books, epubveri 0.31.0, 2026-10-22).** The
whole-shelf A/B moves one book, *Sapiens*: one proposal, applied, `OPF-029`
cleared, nothing else changed in any book. Its one remaining error is a
`width` attribute, which epublift's upgrade converts; with it, the book is
valid after their pipeline, as their note measured.

## RSC-007 — a `<script src>` whose file is not in the book

> **Written spec-first on 2026-10-16**, on epublift's report. The finding shape
> is what epubveri 0.28.0 reports on the shelf inputs.

**Finding.** `opf.content_document.reference_missing_resource`, with
`params[0]` = the `src` value as written and `element_path` naming the
`<script>`:

```
OEBPS/toc.xhtml   /h:html[1]/h:head[1]/h:script[1]   params ["../js/kobo.js"]
```

**The shape.** On the shelf it is one thing in four books: a Kobo reference left
behind in an EPUB 3 `toc.xhtml` whose `js/kobo.js` is not in the container.

```xml
<script type="text/javascript" src="../js/kobo.js"></script>
```

The same document also draws `OPF-014` (it uses `scripted` but its manifest
item does not declare it), which `fix.content_properties` answers by declaring
`scripted`. The document has no other script.

**Fix** (`fix.missing_script`, ConfirmNeeded). Delete the `<script>` element,
with the whitespace before it. If the document then needs no `scripted`
property, remove that token from its manifest item (and the `properties`
attribute, if it was the only token), whether the book declared it or
`fix.content_properties` just did. One proposal per document.

**Why it is one repair.** The file is not in the book, so in any reading system
that resolves the URL inside the container the script has never run. An element
with an empty body and an unresolvable `src` contributes nothing a reader sees.
Repointing it is `fix.reference_wrong_path`'s job, and only when the file is in
the book under another path; here it is in the book under no path.

**Whether the document still needs `scripted` is the detector's answer.**
epubveri decides what makes a document scripted, and this crate does not keep a
copy of that rule. The fix is tried on a copy of the book: delete the
element(s), remove the token if declared, validate. If the copy still reports
the document as using `scripted`, something else needs it, and the token is
left alone. The proposal is made only if, in the copy, no `(id, rule)` count
rises anywhere in the book.

**When it declines.**

- The reported element is not a `<script>` with a `src` (the rule covers every
  reference: an `<img>`, a link).
- The element has content other than whitespace: deleting it would drop code.
- A file with the same name is in the container under another path. Then the
  reference may be stale rather than dead, and that is a different repair.
- In the trial copy, any `(id, rule)` count rises.

**Why ConfirmNeeded, not AutoSafe.** The "never ran" argument holds for a
reading system that resolves the reference inside the book. These are Kobo
references, and whether a Kobo device supplies its own `kobo.js` to a book
that names it is not something we can measure here. The user decides.

**Measured (562 books, epubveri 0.28.0, 2026-10-16, before building).** 4 shelf
inputs, one dead `kobo.js` each, all EPUB 3 (*Kendime Düşünceler*, *İslamın
Siyasallaşması*, *İmparatorluğun En Uzun Yüzyılı*, *Türklerin Tarihi 2*). Each
has three errors: this one, `OPF-014` (`scripted`) and `NCX-001`. epubsana
already repairs the other two, so after repair this is each book's last error.

## RSC-007 — a `@font-face` whose font file is not in the book

**Finding.** `RSC-007` / `css.font_face.missing_target`, **Error**, reported on
the stylesheet: *reference to a resource missing from the publication: 'X'*, with
`params[0]` = the url as CSS reads it, escapes resolved and not made absolute
(`../Fonts/arial.ttf`; `url(a\26 b.ttf)` gives `a&b.ttf`). Since epubveri 0.33.0
a test on its side pins this shape (id, key, location, decoded `params[0]`) as
the contract this fixer dispatches on.
epubveri resolves the url against the stylesheet's own directory and checks it
against both the manifest and the container's name index, so a finding here means
the file is genuinely absent, not merely undeclared.

**Fix** (`fix.font_face_missing_target`, ConfirmNeeded). Delete the whole
`@font-face` rule, with the whitespace that preceded it. One proposal per
stylesheet.

    @font-face {
      font-family: "Arial";
      src: url(../Fonts/arial.ttf);
    }
    →
    (removed)

**Why it is safe.** The font does not load today and cannot: the file is not in
the book. Any `font-family: Arial` elsewhere already falls back to whatever the
reading system substitutes, and deleting the rule that never applied changes
nothing about what a reader sees. The repair removes a declaration that declares
a resource the publication does not contain — the same shape as the dangling
`<guide>` reference, in a different file format.

**The guard that makes it determinate, and the objection it answers.** This
family was ranked "buildable in principle, blocked on a decision" for days, on
this reasoning:

> a `@font-face` block can carry several `src` entries, and dropping the whole
> block when only one is missing would throw away a font that works — so the
> fixer needs to edit *within* a block

Correct, and worth measuring rather than assuming: **every affected block on the
157-book shelf holds exactly one `url()`.** All 21 findings across 5 books. So
within-block editing is not needed for any real case, and the fixer **declines
any block holding more than one `url(`** rather than pretending to handle it.
That decline is the whole of the difficulty, and it costs nothing here.

**It does not open the `css.*` family, and must not be read as doing so.** The
rest of that family — a malformed selector, a malformed declaration value, an
unterminated stylesheet — is open-ended in exactly the way this member is not.
This is a *deletion of a whole rule identified by a resource that is absent*.

**How the rule is found (styloria, 2026-10-21).** Until 0.33 this fixer matched
braces by text, so a `}` in a comment or a string cut a rule short, and a
`url(` in a comment counted as a source. It now asks styloria, which **locates
and never decides**: epubveri still says which url is missing (`params[0]`),
styloria only says where the `@font-face` rules are and which urls each holds.

- The stylesheet is decoded the way epubveri decodes it
  (`styloria::decode_stylesheet(bytes, None, None)`: BOM, then a byte-exact
  `@charset`, then UTF-8), so the text we parse is the text epubveri parsed.
- `parse_stylesheet` gives every `@font-face` at-rule at any depth (inside
  `@media` too) with its span; the deletion is that span plus the whitespace
  before it, as before.
- The rule's urls are `block_url_references` of its block: `url()`, `url("…")`,
  `src()`, and a bad-url token all count; a url in a comment does not.
- The rule's one url must **equal** a reported `params[0]`: styloria's value
  for it, escapes resolved as epubveri resolves them. Until 0.35 this was a
  substring match on the url's source text, which took a working
  `url(myfont.ttf)` for a missing `font.ttf` and deleted both rules, and never
  matched an escaped url (epubveri found both by reading the code,
  2026-10-22). For a `<bad-url-token>`, which styloria gives as source text,
  the value is the text inside `url(…)` trimmed of CSS whitespace; where
  epubcheck's reading stops earlier (a `)` inside) or the name is escaped
  (`\75rl(`), the two differ and the rule is declined.
- The stylesheet is written back with `encode_stylesheet` in its own encoding
  and with its own BOM, so every byte outside the deleted rule is unchanged.
- **Inline `<style>` in a content document** (9 of the 27 proposals on the
  562-book shelf, 2026-10-21; all 27 are byte-identical to the text matcher's): each `style` element whose only child is one text
  node is parsed on its own. Its raw bytes must hold no `&` and no `<`, so that
  they are exactly the CSS text and a span maps back to the document by an
  offset. A `<style>` with an entity, a CDATA section or a comment in it is
  declined.

**When it declines.**

- **More than one url in the rule** — a second source may be present and
  working, and choosing which line to cut is editing CSS rather than deleting a
  dead rule. 0 cases on the shelf. (`local()` is not a url and does not count,
  as before; see the open question below.)
- **A rule whose block is never closed** (runs to the end of the input), or one
  holding a nested rule. Nothing on the shelf is shaped that way.
- **A stylesheet that does not decode losslessly** — an encoding styloria does
  not decode (Shift_JIS, GBK, …), or a byte that does not decode in the one it
  chose. Writing such a file back would replace those bytes with U+FFFD. Shelf:
  878 of 878 stylesheets are UTF-8, 0 lossy (styloria, 2026-10-21).
- A reported url that does not appear inside a `@font-face` rule at all.

**Open question, not decided here: `local()`.** `src: local(Arial),
url(missing.ttf)` holds one url, so the whole rule is deleted, although a
reading system that has Arial installed would have used it. That was true of
the text matcher too and is unchanged. To be measured before anyone decides.

**An unquoted url with a space, a quote or a parenthesis in it (epubveri 0.21+).**
`url(f/a b.ttf)`, `url(f/q'q.ttf)` and `url(f/p(p.ttf)` were a CSS syntax error
(CSS-008) and nothing else up to epubveri 0.20, so nothing was proposed. From 0.21
epubveri reads them as epubcheck does, up to the first `)`, and a missing font
there draws RSC-007 like any other. The fix is the same deletion, and is safer
here if anything: CSS calls such a url malformed, so no browser would load it
even if the file were present. `params[0]` is epubcheck's reading, the text
inside `url(…)` up to the first `)` and trimmed, which is what the equality
compares; a test runs all three shapes through the real detector. 0 cases on
the shelf (2026-10-02).

**Measured (2026-08-13): 21 findings across 5 books**, error severity — the only
error-severity candidate left on this shelf, and so the only remaining fixer that
can move the `--goal valid` line. It does: **one book goes fully valid** that did
not before.

**It clears more than its own rule, and the arithmetic is worth writing down so
nobody reads it as a regression.** Deleting a `@font-face` removes the whole
declaration, so on those five books:

| rule | severity | before → after |
| --- | --- | --- |
| `css.font_face.missing_target` | Error | **21 → 0** (its own) |
| `css.font_face.leaks_container_root` | Error | **8 → 0** (same rules; the url both escaped the container root *and* was absent) |
| `css.font_face.declared` | Usage | **21 → 0** (a deleted declaration is no longer declared) |

Nothing else moves, and the shelf audit reports zero introduced findings.

## RSC-007 — a background image that is not in the book

**Finding.** `RSC-007` / `css.url.missing_resource`, **Error**, reported on the
stylesheet (or on the content document, for an inline `<style>`): *references a
missing resource 'X'*. `params[0]` is the url as CSS reads it, with escapes
resolved. epubveri reports it only when the target is in neither the manifest
nor the container: a file that is declared but absent draws RSC-001 on the
manifest instead.

**Fix** (`fix.css_missing_image`, ConfirmNeeded). Replace the `url(…)` with
`none`, in `background-image`, `background` and `list-style-image`. One
proposal per file.

    h3 { background: url('../Images/underline.gif') no-repeat bottom center; }
    →
    h3 { background: none no-repeat bottom center; }

**Why it is safe, and why there is exactly one repair.** CSS Backgrounds 3
(§3.1, `background-image`): an image that fails to download or cannot be
displayed "still counts as an image layer, but draws nothing", and `none` is
also a layer that draws nothing. So the image draws nothing today, and `none`
draws nothing too. For `list-style-image`, CSS Lists 3 uses the marker set by
`list-style-type` both when the image is invalid and when it is `none`. The
other repairs are not equivalent:

- **Deleting the declaration** lets an earlier rule's image win the cascade.
- **Fixing the path** is a guess.

**What it does not touch.**

- **Any other property.** `content: url(x)`, `cursor`, `border-image`,
  `mask` and the rest each treat a failed image differently. The
  `list-style` shorthand is excluded too: `list-style: none` also sets
  `list-style-type` to `none`, which would remove the marker.
- **A url nested in a function** (`image-set(url(a.png) 1x)`): `none` there
  makes the whole declaration invalid. Only a url that is a top-level value
  of the declaration is replaced. Its span must be the span of one of the
  declaration's own component values.
- **A `<bad-url-token>`** (`url(a b.png)`): CSS drops that declaration
  already, so `none` would make a dead declaration live. Also a `src()`
  function or a bare string.
- **A declaration outside a style rule** (`@page { background: … }`, a
  descriptor).

**When it declines.**

- A file with the url's file name is in the container under another path,
  compared after percent-decoding and without a query or fragment.
  Then the reference may be stale rather than dead, and that is a different
  repair (as in `fix.missing_script`).
- The stylesheet does not decode losslessly (`text_round_trips`).
- For an inline `<style>`: the element's only child must be one text node
  with no `&` and no `<`, as in `fix.font_face_missing_target`.

**How the url is matched.** By equality between styloria's decoded value
and `params[0]`, as in `fix.font_face_missing_target` since 0.35.

**Measured after building (562 books, epubveri 0.33.0, 2026-10-23).** All 3
are proposed and applied, the finding is gone, and no other `(id, rule)`
count moves in any book. *Ruhlar Evi* ends the repair with no finding at
all. The other two keep errors this fixer does not address.

**Measured (562 books, epubveri 0.33.0, 2026-10-23, before building).** 3
books, one finding each, all in a style rule:

- *Ruhlar Evi*: `h3 { background: url('../Images/underline.gif') no-repeat
  bottom center; }`. This is the book's last error.
- *101 Soruda Kuantum* and *Üç Cisim Problemi*: `body { background-image:
  url(watermark.jpg); }`.

## CSS-008 — a stylesheet whose `{` is never closed

**Finding.** `CSS-008` / `css.stylesheet.unterminated`, **Error**, reported on
the stylesheet (or on the content document, for an inline `<style>`), at the
`{` that is never closed: *CSS syntax error: '{' is never closed*. `params` is
empty.

**Fix** (`fix.css_unterminated`, ConfirmNeeded). Append one `}` per open block
at the end of the text: the end of the file, or the end of the `<style>`
element's text. Every other byte stays as it is. One proposal per file.

    h6 {
       text-align: center;
    →
    h6 {
       text-align: center;
    }

**Why it is safe, and why there is exactly one repair.** CSS Syntax 3 closes
every open block at the end of input (§5.4: "this is a parse error; return
the block"). So a reading system that follows the spec already reads the
stylesheet as if the `}` were there, and appending it changes nothing a reader
sees. The repair adds what the parser already assumes. Any other place for the
`}` would change which rules a block holds, so it would change the rendering
and would be a guess.

**The case where the author probably meant something else.** In one shelf
book, `@media amzn-mobi {` is never closed, so every rule after it sits inside
that block. The author probably meant to close it much earlier. Appending
the `}` at the end keeps those rules where a reader puts them today, inside
the block. It does not move them to where the author may have meant them to
be: that would change what readers see, and it would be a guess. The proposal
is ConfirmNeeded, so the user sees the file and decides (Baris, 2026-10-23:
"keep what the reader sees" is the rule).

**What proves the equivalence: one test, and why one is enough.** A `}`
appended at the end does one of two things:

- it closes a block, and then the tree is the one CSS already builds when it
  closes that block at the end of input;
- or a token takes it in (a comment, a string, an open `[` or `(`), and then
  the block is still open.

So the repair is proposed only if styloria, parsing the repaired text, finds
**no block left open**. An open `[` or `(` is declined this way too.

The first design had two more guards: "every open bracket is a `{`" and "the
repaired text serializes as the original does". A mutation check showed that
no test could make either of them fire. A probe then cut each of the shelf's
878 stylesheets at 40 random points (35,120 cuts, 2026-10-23):

- 28,429 cuts left a block open;
- in 27,333 of them the appended `}`s closed every block. In every one of
  those, the repaired text serialized exactly as the original did, and no
  `[` or `(` was open.
- In the other 1,096, the `}` was taken into a token and the block stayed
  open, which the one guard declines.

Neither extra guard ever fired on its own, so both were removed. Two guards
remain, as for every CSS write:

- the stylesheet must decode losslessly (`text_round_trips`);
- for an inline `<style>`, the element's only child must be one text node with
  no `&` and no `<`, as in `fix.font_face_missing_target`.

**Measured (562 books, epubveri 0.33.0, 2026-10-23, before building).** 2
books, one finding each:

- *Beş Sevgi Dili*: the last rule, `h6 { … }`, has no `}`.
- *Homo Deus*: the `@media amzn-mobi {` at line 25 is never closed.

Both are external stylesheets, and both end outside a comment or a string.

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

**A fragment that spells the id percent-encoded (epublift, 2026-10-21).** A
reading system percent-decodes a fragment before matching it against an id (URL
Standard; epubveri resolves links the same way), so `href="03.xhtml#k10%20"` is
a reference to `id="k10 "`. Before 0.33.1 the scan looked for `#k10 ` only, did
not see it, and the rename left that link dangling. Now every `#` followed by a
run of fragment characters and `%` that contains a `%` is decoded — a `%` not
followed by two hex digits stays as written, as in the URL Standard, and a run
that does not decode to UTF-8 names no id — and, when it equals the old id, it
is resolved and rewritten like any other reference. The new id is written
unencoded: a sanitized NCName needs no escaping. The literal match also stops
at a `%` now, so `#k10%20` is never taken for a reference to an id `k10`.
Measured (2026-10-21): one book on the 562-book shelf, the only id with stray
whitespace on it, whose footnote back-link is the one reference. Not found: a
spelling that mixes a raw character no fragment may hold with escapes
(`#a b%41`); no such link is a valid URL, and the post-repair detect still
reverts a rename that leaves one dangling.

**When it declines.**

- **An occurrence it cannot classify.** A `#value` that is not inside a quoted
  attribute value — an id selector in a stylesheet, a string in script, a mention
  in prose — cannot be rewritten with confidence, so the whole rename for that id
  is abandoned. This is the safety net, and it is why the fixer can promise that
  a rename never leaves a reference behind.
- **A reference that climbs out of the container** (`../` past the root).
  A percent-encoded path is **not** a decline since 2026-10-08: it is decoded
  with epubveri's `opf::resolve` only to decide which document it names, and
  the rename rewrites the fragment alone, so no encoding is ever re-emitted.
  (The 544-book shelf has 1,040 such links in 10 books, none of them pointing at
  an invalid id, so no proposal moved.)
- **A selector whose match the rename would change (styloria, 2026-10-21).**
  The text scan above sees `#09`, but not `#\30 9` (the same id, escaped) or
  `[id^="0"]`, and a rename past either silently changes what a stylesheet
  styles. Every `.css` entry, and every `<style>` element in a markup entry, is
  parsed with styloria, and `selector_references` lists every id and attribute
  selector at any depth (escapes resolved). The id is declined when, for any of
  them, *matches the old name* differs from *matches the new name*: `#09`,
  `#\30 9`, `#id_09` (the new name), `[id="09"]`, `[id^="0"]`. One that matches
  both or neither (`[id]`, `[id*="9"]`) is left alone, since the rename cannot
  change it. Attribute selectors are compared per Selectors 4 (`=`, `~=`, `|=`,
  `^=`, `$=`, `*=`); without an `s` flag the comparison is ASCII
  case-insensitive, which can only decline more. A selector in a rule a browser
  drops (`dropped`) styles nothing and is ignored. The text scan stays: a rule
  with a token error is not listed by styloria, and `#09` in it still declines.
  A stylesheet that does not decode losslessly, or a markup entry with a
  `<style>` that does not parse, declines every rename in the book.
  Shelf (562 books, 2026-10-21): 878 stylesheets, 2,349 id selectors, 0
  escaped, 0 attribute selectors on `id` (`.css` entries only; inline `<style>`
  not counted). The whole-shelf plan is identical before and after, so this
  guard is proven by its tests (each mutation-checked), not by the shelf.
- **A reference inside a stylesheet that does not decode losslessly.** The
  fragment would be rewritten by writing the stylesheet back, and that cannot
  be done without U+FFFD. Every stylesheet is now written in its own encoding
  (`encode_stylesheet`).
- **An id containing `%`.** A link that spells it as written is decoded by a
  reading system into another value (`#a%41` means `aA`), so which links mean
  this id cannot be read off the text.
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

## OPF-001 — a package version that is not an EPUB version

**Finding.** `opf.package.unrecognized_version`. `params[0]` is the declared
version. On real books it is `1.0`, written by InDesign and kept by Sigil.

**This finding hides every other one.** epubveri, like epubcheck, chooses its
checks by the package version and runs none for a version it does not know. A
book with `version="1.0"` therefore reports this one error and nothing else.
"One error" here means "never checked", not "nearly valid".

**Fix** (`fix.package_version`, ConfirmNeeded). Set the attribute to `2.0`,
keeping the quote character the file used. Nothing else in the package changes.

**Why 2.0 is not a guess.** The package is in the EPUB namespace
(`http://www.idpf.org/2007/opf`), which only EPUB 2 and EPUB 3 use; OEB 1.x
had its own. EPUB 3 requires a navigation document, and the manifest declares
none, but it does declare an NCX. Only EPUB 2.0 is consistent with what the
book itself says.

**What happens next, and why it is not a regression.** The next validation is
the book's first, and it can find defects that were always there. On the test
shelf one book went from 1 error to 29. `repair()` does not undo this fix for
them: a fix that lets the detector see what it could not see before is treated
like a fix that clears a fatal, and what appears is accepted as revealed. A
later fix that raises the same finding further is still undone. The run ends
with a note saying how many findings appeared and suggesting a second run; the
fix's own title says the book will be checked for the first time.

**When it declines.**

- A package outside the EPUB namespace. An OEB 1.x package is a different
  format, and epubcheck's own fixture for one expects this finding and nothing
  else.
- A manifest item with the `nav` property. EPUB 3 is then possible, and the
  version is a choice.
- No NCX item. Nothing in the book says which version it was written for.
- A `version` in the file that is not the one reported.

**Measured.** 544 books, epubveri 0.19.1: six books carry the finding, and all
six are proposed. After the fix, and a second run on the output, four of them
are valid. The other two keep an empty `<dc:identifier>`, which only the
author can fill.

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
  Note the standing project question about generated identifiers is
  therefore still unanswered and still not needed.
- **The single candidate is itself empty.** Attaching the id would clear
  `OPF-030` and raise the empty-identifier finding in its place, which is not a
  repair. Empty means XML whitespace only; a no-break space is a value.
- **The `unique-identifier` value is not an NCName as written**
  (`epubveri::xmlname::is_ncname`). The repair writes it as an `id`, so it must
  be one. A padded value names nothing whatever we write, because epubveri
  trims the `id` side only; one carrying a no-break space, or starting with a
  digit, resolves only through an `id` that is itself RSC-005.
- A package document that doesn't parse.

**Measured.** 10 findings across 10 books; **3 repaired, 7 declined**. That ratio
is the honest headline: this fixer's value is that it refuses to guess about a
book's identity six times out of ten, and the three it does repair need no guess
at all.

---

## RSC-005 — the four `playOrder` faults (an addendum to NCX internal consistency)

epubveri reports four separate `playOrder` rules and they interlock, so
satisfying one naively breaks another:

- `ncx.play_order.duplicate` — **different** targets sharing a number.
- `ncx.play_order.target_mismatch` — **one** target reached by elements carrying
  different numbers.
- `ncx.play_order.gap` — a number with no predecessor.
- `ncx.play_order.no_origin` — **no element carries `playOrder="1"`**, so the
  sequence never starts.

`fix.ncx_play_order` reassigns the whole NCX the way the format defines:
**1-based, dense, in document order, and elements naming the same target share
the first number that target was given.** That satisfies all four at once, and
no assignment satisfying all four differs from it.

**`no_origin` was missing from the dispatch list until 2026-08-20, and the way it
hid is worth knowing.** The repair had always covered it — a 1-based dense
renumbering starts at 1 by construction — but the fixer only ran when one of the
*other three* rules appeared. A book whose only fault was the missing origin got
no proposal at all, and nothing in the code or its tests could show that: the
repair logic was right, the trigger was incomplete. Two shelf books were in
exactly that state, both with a single `<navPoint>` carrying `playOrder="0"`.

Note that epubveri compares the origin as a **string** (`@playOrder='1'`, per the
Schematron) while the gap check compares numerically — so a padded `playOrder="01"`
draws `no_origin` and no `gap`, and would likewise have gone unrepaired.

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

## RSC-007 — an NCX `<content src>` whose path no longer resolves

**Finding.** `opf.ncx.content_src_missing_resource`. A `<navPoint>`'s `<content
src>` names a file that is not at that path; `params[0]` carries the raw `src`.

**Fix** (`fix.ncx_src_wrong_path`, ConfirmNeeded). If **exactly one** container
entry has that basename, rewrite the path to point at it, relative to the NCX.
The fragment, if any, is carried across unchanged.

**It is the NCX member of a family already closed elsewhere**, and shares its
argument and its code with `fix.reference_wrong_path`: the same
`repointed_reference` decides the target, so the guards cannot drift between the
two sites. "Missing resource" is now repaired at the content document, the
`<guide>`, the manifest item, `@font-face` and the NCX; `css.url.missing_resource`
is the one member still open.

**Why it's safe.** The reference names a file by basename, exactly one entry in
the container carries that basename, so that entry is the file it meant — not a
guess. Where the `src` carries a fragment, that fragment must already exist in
the chosen target, so the repair cannot trade this finding for a dangling
`RSC-012`; the check is also evidential, since a same-named file that merely
happened to be elsewhere would not carry the right anchor.

**When it declines.** A basename matching nothing (the file is genuinely absent)
or several entries (which one it meant is a guess); an external URL, a bare
hostname, placeholder junk, or a percent-encoded path; a fragment the target does
not define; and a `src` that cannot be found as a quoted attribute value in the
NCX.

**Measured (385 books, 2026-08-21).** 46 findings across 5 books, of which
**exactly one is repairable** — a Calibre book whose NCX points at
`OEBPS/Text/titlepage.xhtml` while the file sits at the container root. Every
other finding names a file that is simply not in the book, which is the shape
this rule mostly has.

**This rule was closed and re-opened by the corpus, for the second time in this
catalogue.** `ncx_src_probe` measured it on 2026-08-12 across 157 books and found
**zero** repairable cases — all seven findings pointed at absent files, so there
was nothing to repair toward and the rule was left alone. The shelf then grew to
385 and produced one case with the determinate shape. The same thing happened to
`opf.content_document.reference_missing_resource` on 2026-08-07. **Re-probe a
closure when the corpus moves; a closure is a statement about the books you had.**

---

## RSC-012 — a fragment that is real but has moved to another document

**Finding.** `opf.content_document.dangling_fragment`. A link's `#fragment` is
not defined in the document the link resolves to. `params` carries
`[fragment, resolved target]`.

**Read `params[1]` as evidence, never as a needle.** It is the target
*resolved* and NFC-normalised, so it is not a string the referring document
contains — the document holds a relative href. `params[0]`, the fragment, is
the findable half, and the reference is located by the attribute value ending
in `#{fragment}`.

**Fix** (`fix.fragment_wrong_path`, ConfirmNeeded). If **exactly one** container
entry defines that id, write that entry's path in front of the fragment,
relative to the referring document. When the id turns out to live in the
referring document itself, write the bare `#fragment` instead of a path to the
document's own name — same-document is what a bare fragment means, and it is
the form an editor would have left.

This is the third member of the family beside `fix.reference_wrong_path` and
`fix.ncx_src_wrong_path`, and it **shares their machinery** — `relative_path`
and `quoted_attr_span` — rather than re-deriving it. It cannot share
`repointed_reference`: those two ask *which entry has this basename*, and this
one asks *which entry defines this id*. Different question, same discipline —
exactly one answer or decline.

**What the corpus is.** An editor split a file at a page break and left a
footnote link and its return link as bare `#frag`s pointing into the wrong half.
Verified by hand in *Harika Piç*: `..._split_008.htm` links to `#bookmark1`
while `bookmark1` now sits in `..._split_025.htm`, and `..._split_025.htm` links
back to `#footnote1`, which stayed behind in `..._split_008.htm`.

**Guards, and every one of them is a measurement rather than an argument**
(`frag_spec.rs` / `frag_reprobe.rs`, 474 books, epubveri 0.14.3):

| guard | why | what it declines here |
| --- | --- | --- |
| exactly one entry defines the id | several is a guess about the author's intent | 71 findings / 8 books |
| the id exists somewhere in the book | nothing to repair toward; only "drop the fragment" is available, and that loses the author's target | 109 findings / 19 books |
| the target is in the spine | epubveri 0.12/0.13 added `hyperlink_target_not_in_spine`; without this the repair could author it | 0 of 22 |
| the reference is visible as a whole quoted attribute value | never rewrite from inside a longer string | 0 of 22 |
| the document the link names is in the container | see below | 0 findings / 544 books (epubveri 0.19.1) |

**The missing-target guard was added on 2026-09-30, when the detector widened
the rule.** From epubveri 0.19.0, `dangling_fragment` also fires for a fragment
into a document the manifest declares and the container lacks, as epubcheck
does. "The anchor moved" was measured only on targets that exist. With the
target gone, an id defined once elsewhere (`note1`, `p1`) is as likely to be a
coincidence as a move, and the book's real defect is the missing file, which
`RSC-001` already reports. This check reads `params[1]` as evidence, so a
finding without it declines too. It declines the fragment for the whole
referring document, because the rewrite reaches every reference to it.

**The last three decline nothing on this shelf, and they are still written.** That
is the point of recording the number: a guard whose population is zero is
carried by argument, and the argument is that each names a finding the repair
would otherwise author. `fix.content_properties` is the standing reminder — it
cleared an `OPF-014` and wrote an `RSC-005` in its place, at an error count that
did not move.

**There was a third, and mutation-testing deleted it.** A separate "is the
target a manifest item" check read like safety and could never fire: the spine
set is built by resolving each `<itemref idref>` through the manifest, so an
undeclared entry cannot reach it. Removing that half of the condition broke no
test — which is the correct answer, not a gap in the tests. Every other guard
here fails a test when removed.

**Measured population.** 22 findings / 6 books have the determinate shape,
becoming **10 proposals** (one per document). 2 of the 22 take the bare
`#fragment` form. Identical at 444 books and at 474, so the shape is the
corpus's and not one book's.

**Why it was not built sooner, which is the part worth keeping.** This rule was
dismissed on 2026-08-12 with *"worth a probe, but it is seven findings"* —
**by size, not by argument**, which is exactly the dismissal that a growing
shelf turns into a mistake. It came back at 24 books / 201 findings.

---

## RSC-005 — `lang` and `xml:lang` disagree

**Finding.** `opf.content_document.lang_xmllang_mismatch`, message *lang and
xml:lang attributes must have the same value*. `params[0]` is the `lang` value and
`params[1]` the `xml:lang` value, both **as written**: epubveri compares them
untrimmed, as epubcheck does, so `lang=" en"` beside `xml:lang="en"` is a real
mismatch (it once trimmed both, and our match followed it). **EPUB 3
only** — epubcheck asserts it in `epub-xhtml-30.sch` and XHTML 1.1 declares the
two attributes independently, so a disagreeing EPUB 2 book is valid and is never
reported.

**Fix** (`fix.lang_xmllang_mismatch`, AutoSafe). When exactly one of the two is
**empty**, write the other one's value into it. Every other attribute, the quote
style and the element are untouched.

**Why it's safe, and why it is narrower than the rule.** An empty attribute
carries no language claim — HTML treats `lang=""` as *unknown*, which is
precisely the absence of information — so copying the populated sibling into it
destroys nothing and states what the document already says once. The book's
declared language does not change; one of the two spellings simply stops
contradicting the other.

**It declines when both values are non-empty**, and that is the whole judgement
in this fixer. `lang="en"` against `xml:lang="fr"` is two real claims, and
choosing between them is an editorial act about what language the text is in —
which we cannot know and must not guess. Such a book is left alone.

**Which side to fill is not a choice.** With one side empty there is exactly one
value in play, so the repair has no degrees of freedom — the same reasoning that
made `fix.empty_lang` fill an empty root language rather than delete it.

**When it declines.** Both values non-empty (above); either value padded with
XML whitespace, a whitespace-only one included — copied as is the padding lands
in a second attribute, trimmed the two still differ, so there is no single clean
repair; the document won't parse; or
no element carries both attributes with the reported pair, in which case nothing
is edited rather than guessed at.

**Measured (375 books, 2026-08-20).** 4 findings in 1 book, every one of them the
same shape: `lang="tr"` against an empty `xml:lang`. The both-populated shape does
not occur on this shelf — so the decline branch is carried by argument and a unit
test, not by a book.

---

## RSC-005 — an anchor target wrapped around a link

**Finding.** Two rules, one per version, both `RSC-005` errors with empty
`params` — the finding gives the file and nothing else, so the element is
re-located by parsing, as the other structural fixers do:

- EPUB 2: `htm.epub2_dom.nested_anchor`, *The "a" element cannot contain any
  nested "a" elements*.
- EPUB 3: `opf.content_document.no_interactive_in_a__a`, from epubcheck's
  Schematron pattern *no-interactive-in-a--a*: *interactive content (a "a"
  element) must not appear inside an "a" element*. The same pattern also
  reports an `<a>` inside a `<button>`, with the same rule and its own message.
  That shape is not repaired here: the fixer only ever edits an `<a>` that
  contains another `<a>`, so a document whose only finding is an `<a>` in a
  `<button>` gets no proposal, and one that has both keeps the button finding.

Both rules go to the same fixer under the same id (`fix.nested_anchor`), as the
EPUB 3 empty Dublin Core element did in 0.29.0: a caller that already approves
it gets the EPUB 3 repair unchanged. The EPUB 3 rule appears on the shelf only
after an EPUB 2 → 3 upgrade, when the EPUB 2 finding becomes this one.

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

**Measured (562 books, 2026-10-16).** EPUB 2: 111 findings in 10 books, every
one cleared, all the footnote shape above. (The first measurement, 6 findings in
one book on 2026-08-20, came before the shelf grew.) EPUB 3, after epublift's
EPUB 2 → 3 upgrade (cli-v4.0.0): the same 111 findings in the same 10 books,
every one inside an `<a>` (none in a `<button>`); before 0.30.0 none was
repaired, after it all 111 are, no other count rises and none of its edits is reverted.
The claim is still the mechanism: it moves an id off a wrapper that exists only
to hold it, and refuses every case where the wrapper carries anything else.


---

## RSC-005 — an optional `<nav>` whose list is empty

> **Written spec-first on 2026-09-09, then built the same day.** Every claim
> below was an answer the detector gave to a fixture *before* any code existed;
> the *Measured* section at the end is what the shelf said afterwards, and it
> did not move a single one of them.

**Finding.** `RSC-005` / `navdoc.ol.empty`, **Error**, *element "ol" incomplete;
missing required element "li"*. An `<ol>` in the EPUB 3 navigation document has
no `<li>` children. `params` is **empty** and the location is the navigation
document, so the finding names the file and nothing else.

**Measured (474 books, epubveri 0.13.7): 3 books, 3 findings, one shape.**

```xml
  <nav epub:type="landmarks" hidden="">
  <ol></ol>
</nav>
```

All three sit last in `<body>`, after a populated `toc` nav, and all three are
Calibre output — one is in `calibre/`, two in `commercial/`, but the single
quotes in the XML declaration and the identical indentation give away one
producer wearing three covers. **The shelf therefore cannot tell us whether the
shape generalises**, and this entry does not claim it does.

**Fix** (`fix.navdoc_empty_nav`, ConfirmNeeded). Delete the **whole `<nav>`
element**, with the whitespace that preceded it.

**Why the whole `<nav>` and not the `<ol>`.** Measured, not reasoned: deleting
only the empty `<ol>` and leaving the `<nav>` standing produces *two* new
findings — `navdoc.nav.missing_ol` (`RSC-005`, error: *element "nav" is
incomplete; it requires an "ol" element*) and `navdoc.nav.not_flat` (`RSC-017`,
warning). That is the trade `fix.manifest_dangling_item`'s guards exist to
refuse. Deleting the whole `<nav>` leaves the detector with nothing to say about
the navigation document at all.

**Why it is safe.** A `landmarks` or `page-list` nav is optional in EPUB 3, and
one whose list is empty states nothing — the same argument
`fix.empty_metadata_element` makes for an empty `<dc:*>`. There is no value in it
to lose, and no reader-visible content is removed: the element is a list with no
entries.

**`params` cannot tell the shapes apart. The finding can — and this fixer does
not read the part that does.** Corrected 2026-09-09, after v0.14.0 shipped with
the weaker claim: `navdoc.ol.empty` is emitted through `push_node`, which sets
`element_path`, and on all three shelf books that path is
`/h:html[1]/h:body[1]/h:nav[2]/h:ol[1]` — the exact `<ol>`, distinguishing a
nav's own list from one nested inside an `<li>`. So the discriminant exists and
we decline to consume it, which is a choice with a cost, not an absence.

**Why the choice is nonetheless safe here, and this is the real argument.** The
proposals are a **strict subset of the findings, by construction**: for a typed
`<nav>` whose single element child is an `<ol>`, epubveri's
`check_nav_content_model` always reaches `check_ol`, and `check_ol` always
reports an `<ol>` with no `<li>`. Every element this predicate selects is
therefore an element epubveri reported. That is a property of the two grammars
agreeing, checkable in upstream's source (`src/navdoc.rs`), and it is what makes
the file dispatch safe — *not* the vaguer "the predicate matches the grammar",
which is what the first version of this entry said.

**Keep the subset property in mind if either side moves.** It is the whole of the
safety argument, and it is an argument about someone else's code. If upstream
narrows when `check_ol` is reached, this fixer could begin acting where nothing
was reported — the failure `fix.empty_metadata_element` actually had. Consuming
`element_path` would remove the dependency entirely, at the cost of a path
matcher (roxmltree has no XPath) and a new coupling to upstream's path
computation; it is the right change if this family grows.

Four different books produce the identical message with the identical (empty)
params:

| shape | what the detector says | what the fixer must do |
| --- | --- | --- |
| optional nav, own `<ol>` empty | `navdoc.ol.empty` | **repair** |
| `toc` nav, own `<ol>` empty | `navdoc.ol.empty` | **decline** |
| empty `<ol>` nested inside an `<li>` | `navdoc.ol.empty` | **decline** |
| untyped `<nav>`, `<ol>` empty | *nothing at all* | **never act** |

**When it declines.**

- **The nav is the `toc`.** Measured: deleting it produces
  `navdoc.document.missing_toc` (`RSC-005`, error) — the book would be no more
  valid and would have lost its table of contents. This is the navigation-
  document guard `fix.manifest_dangling_item` already carries, at a third site.
- **The document has no surviving `toc` nav for any other reason.** A book
  already missing one has a larger defect than this; repairing around it is not
  this fixer's business.
- **The empty `<ol>` is not the nav's own direct child.** A nested `<ol>` inside
  an `<li>` draws the identical message, and there the whole-`<nav>` deletion
  would remove a populated table of contents. Dropping just the nested `<ol>` is
  plausibly determinate — an `<li>` whose content is `(a|span), ol?` is valid
  without it — but **there is no case of it on the shelf**, so it is left
  unranked rather than specified. Recorded here so the next probe does not have
  to rediscover the shape.
- **The `<nav>` has no `epub:type`.** Such a nav is *unrestricted* — epubveri's
  own module comment says so, confirmed there against a real fixture, and a
  fixture here confirms the finding never fires. Acting on one would be repairing
  something no detector reported: exactly the file-dispatch failure the NBSP
  deletion was.
- **The `<nav>` carries a heading.** `<nav><h2>Landmarks</h2><ol></ol></nav>`
  draws the same finding, and deleting it removes visible text. Two repairs exist
  — drop the nav, or fill the list — so it is not determinate. Zero cases on the
  shelf; this is a decline on the argument, and it would stay one if a case
  appeared.
- **Anything inside the deleted subtree carries an `id`.** A `<nav id="lm">` may
  be the target of `nav.xhtml#lm` from anywhere in the book, and deleting it
  would author a dangling fragment. This is stricter than necessary — a fragment
  scan like `any_entry_references` would be exact — and it is accepted as
  stricter because the subtree is at most two elements and **no shelf case
  carries an `id` at all**, so the cheap version costs nothing measurable. Revisit
  only if a real book is declined by it.
- A navigation document that will not parse.

**What it does not do.** It never creates a `landmarks` nav, never fills one, and
never touches a nav whose list has entries. `navdoc.li.span_missing_ol`,
`navdoc.nav.missing_ol` and `navdoc.nav.not_flat` are separate rules with
separate arguments and are out of scope here.

**Measured after building (474 books, epubveri 0.13.7).** 3 proposals on the 3
books, one each — exactly the finding count, so nothing was re-derived that had
not been reported, which is the check this fixer's shape most needs. Errors
15,722 → **15,719** and findings at every severity 17,509 → **17,506**: three
errors out, nothing authored at any severity, `regression_audit` introducing
nothing. The whole-shelf plan digest gains exactly three lines and changes
nothing else.

**One book goes fully valid** — *Bitmemiş Öyküler*, 14 errors → **0**, this fixer
being the last of its fourteen. Fully-valid books on the shelf: 72 → 73. The
other two land at 2 errors each and the residue is nobody's mistake to fix here:
an empty `<title>` plus a `file:///` URL in one, two `element "div" is not
allowed here` schema violations in the other.

**Do not read that as the fixer's general worth.** Three findings is a small
population from a single producer, and the fully-valid book is fully valid
because its *other* thirteen defects were already repairable — this one happened
to be the last brick. The durable claim is narrower: `navdoc.ol.empty` was one of
the error-severity rules with no fixer, and it no longer is.

---

## RSC-005 / RSC-025 — an SVG attribute whose name lost its case

> **Written spec-first on 2026-10-14.** The finding shape below is what
> epubveri 0.26.0 reported on the shelf before any code existed.

**Finding.** `svg.attributes.attribute_not_allowed`, `RSC-005` (error) on EPUB 2,
`RSC-025` (usage) on EPUB 3, `violation_kind` none. `params` =
`[attribute, element]`, and `element_path` names the attribute itself:

```
/h:html[1]/h:body[1]/h:div[1]/svg:svg[1]/@viewbox      params ["viewbox", "svg"]
```

The rule key is new in epubveri 0.26.0; before it these findings carried no key,
so no fixer could dispatch on them.

**The shape.** SVG attribute names are case-sensitive, and many are camelCase:
`viewBox`, `preserveAspectRatio`. A tool that treats XHTML as HTML lowercases
them on the way through:

```html
<svg … preserveaspectratio="xMidYMid meet" viewbox="0 0 1207 1920" …>
```

An HTML parser would read those back as `viewBox` and `preserveAspectRatio`.
The WHATWG parsing algorithm does exactly that, through the *adjust SVG
attributes* table. An EPUB content document is XML, though, so a reading system
looks for `viewBox`, does not find it, and the cover image loses its scaling.

**Fix** (`fix.svg_attribute_case`, AutoSafe). Rename the attribute to the name
the *adjust SVG attributes* table gives for it (58 rows, all lowercase →
camelCase, taken verbatim from the HTML standard). Only the bytes of the name
change. The value, the element and every other byte stay as they are. One
proposal per book, because the decision is the same in every file.

**Why it is safe.** The table is the HTML standard's own inverse of this damage.
It is total and one-to-one on its rows: each lowercase name has exactly one
camelCase form, and no two rows share one. So the repair makes the XML
document say what the same markup already means to an HTML parser. Nothing is
inferred about the author, and nothing is deleted.

**When it declines.**

- **A name not in the table.** `alt` on `<image>` (3 shelf books) is the common
  case. It is not a case problem, and its repairs (deleting it, or moving the
  text into `<title>` or `aria-label`) are choices, not one correct answer.
- **The element already carries the camelCase name** (`viewBox` beside
  `viewbox`). Then there are two values, and which one is right is a choice.
- **An attribute in a namespace.** The table applies only to unprefixed
  names, and `x:viewbox` would otherwise share the reported path of the
  unprefixed `viewbox` beside it. (The element is always SVG: the rule is
  only reported on SVG elements, and the fixer finds the node by that path.)
- **A node whose path no finding names**, or a document that does not parse.
  The fixer renames only what the detector reported, so it cannot outlive a
  finding upstream stops making.

**The restored name may itself be disallowed on that element.** No table of
which element takes which attribute is kept here: that would be a second
oracle. An HTML parser would restore the case on any SVG element, but on one
that does not take, say, `viewBox`, the rename changes the file and lowers no
count. So the detector decides. Every planned rename is applied to a copy of
the book first, and **a rename epubveri still reports under its restored name is
declined** (Baris, 2026-10-14). That costs one extra validation, and only for a
book that has a candidate. If every rename is declined, nothing is proposed.

**Measured (544 books, epubveri 0.26.0, before building).** 1 book, 2 findings,
both in the table (`viewbox`, `preserveaspectratio` on the cover's `<svg>`).
That is a thin population. It shows that the shape exists and that the
repair fits it. It does not show how often the shape occurs.

**Measured after building (544 books, epubveri 0.26.0, 2026-10-14).** One
proposal, on that book. Errors 15,880 → **15,878** after repair and findings at
every severity 18,297 → 18,295: both findings cleared, nothing authored at any
severity. The whole-shelf plan digest gains exactly that line. The only other
change is that this book's next proposal moves from `--apply` index 3 to 4. The
trial validation never declined anything here; the guard is pinned by tests on a
`<rect viewbox>`, which epubveri reports again as `viewBox`.

---

## RSC-005 / RSC-025 — an SVG element whose name lost its case

> **Written spec-first on 2026-10-15.** The finding shape below is what
> epubveri 0.28.0 reported on probe books before any code existed. **The shelf
> holds none** (562 books, 0 findings under the rule), so this fixer is
> verified on fixtures only.

**Finding.** `svg.elements.element_not_allowed`, `RSC-005` (error) on EPUB 2,
`RSC-025` (usage) on EPUB 3, `violation_kind` none. `params` =
`[element as written, parent's local name]`, and `element_path` names the
element:

```
…/svg:svg[1]/svg:defs[1]/svg:lineargradient[1]      params ["lineargradient", "defs"]
```

The rule key is new in epubveri 0.28.0, added for this repair after the
attribute twin shipped. Before it the finding carried no key, no `params` and
no path.

**The shape.** The same damage as the attribute case, on element names: a tool
that treats XHTML as HTML writes `lineargradient`, `clippath`, `textpath`,
`foreignobject`. An HTML parser reads them back through the *adjust SVG tag
names* table. An XML reading system does not know the lowercased element, so a
cover loses its gradient, its clipping or its text path.

**Fix** (`fix.svg_element_case`, AutoSafe). Rename the element, in its start
tag and its end tag, to the name the *adjust SVG tag names* table gives (37
rows, all lowercase → camelCase, verbatim from the HTML standard, fetched
2026-10-15). Only the local name's bytes change; a prefix (`s:lineargradient`),
the attributes and the content stay as they are. One proposal per book.

**Why it is safe.** As for the attribute table: it is the HTML standard's own
inverse of this damage, total and one-to-one on its rows. And a case-only
rename keeps every byte offset in the file, so nothing else moves.

**When it declines.**

- **A name not in the table.** The rule fires for any SVG element name the
  vocabulary does not know, so a real typo (`rectt`) arrives under the same
  key. It has no single correct spelling.
- **An element whose path no finding names**, or a document that does not
  parse.
- **A rename epubveri still reports under the restored name**, found the same
  way as for the attribute case: the renames are applied to a copy of the book
  first, and the detector decides. No table of which element may sit where is
  kept here.
- **Every rename in a document where the trial raises any other `(id, rule)`
  count.** A restored name is a known element, so the content model applies to
  it where it stands: `<svg><textpath>` becomes a `textPath` directly under
  `<svg>`, which draws a keyless `RSC-005` that was not there before (probe,
  EPUB 2 and 3). `repair()` would count that as a rise and undo the fix, so it
  is not proposed. The new finding has no path, so it cannot be pinned on one
  element; the whole document is declined, which is the conservative side.

A camelCase sibling of the same name (`linearGradient` beside a lowercased
`lineargradient`) is **not** a reason to decline: they are two elements, not two
values for one, and the rename makes the second one what an HTML parser already
reads it as.

**Order with the attribute fixer.** A lowercased element usually carries
lowercased attributes too (`<lineargradient gradientunits>`), and epubveri
reports those under the attribute rule at a path that runs through the
element's name. Renaming the element changes that path, so
`fix.svg_attribute_case` is planned (and applied) first. The probe confirms the
rename reveals nothing: the attribute finding exists before it, and only its
path changes.

**Prefixes.** epubveri writes `element_path` with the document's own prefix
where there is one (`/…/s:svg[1]/s:lineargradient[1]` for `<s:svg>`), and
binds it in the path's `namespaces`. Both SVG fixers read a reported path
through that map before comparing it with the path they compute. Until
2026-10-15 `fix.svg_attribute_case` compared the prefix by its spelling and so
declined every prefixed SVG silently; found while testing this fixer.

---

## RSC-005 — a `dir` value in capitals

> **Written spec-first on 2026-10-14.** The finding shape below is what
> epubveri 0.26.0 reported on the shelf before any code existed.

**Finding.** `opf.content_document.schema_violation`, kind
`invalid_attribute_value`, `params` = `["dir", value]`, and `element_path`
names the attribute:

```
…/h:body[1]/@dir      params ["dir", "LTR"]
```

**The shape.** `dir` takes `ltr` or `rtl` (EPUB 3 adds `auto`), in lowercase.
HTML compares enumerated attribute values without regard to ASCII case, so a
browser reads `dir="LTR"` as `ltr`. The XHTML grammar compares exactly and
rejects it. On the shelf the shape is calibre output:

```html
<body text="#000000" dir="LTR" class="calibre">
```

**Fix** (`fix.dir_case`, AutoSafe). Replace the value with its ASCII
lowercase, only when that is `ltr` or `rtl`. Only the value's bytes change. One
proposal per book, because the decision is the same in every file.

**Why it is safe.** The lowercased value is what HTML already reads the
capitalised one as, so the direction of the text does not change. `ltr` and
`rtl` are valid on every element that takes `dir`, in EPUB 2 and EPUB 3 alike.
The finding is "invalid value", not "attribute not allowed", so the attribute
is allowed where it stands, and no second check is needed.

**When it declines.**

- **A value whose lowercase is not `ltr` or `rtl`.** `AUTO` is valid only in
  EPUB 3, and a value such as `left` has no single correction.
- **A value that is not exactly what the finding reports**, byte for byte
  between the quotes (padding, an entity reference). That is a different shape
  and the edit would not be the one argued here.
- **A node whose path no finding names**, an attribute in a namespace, or a
  document that does not parse.

**Measured (544 books, epubveri 0.26.0, before building).** 2 books, 12
findings, all `<body dir="LTR">`. Both books also carry `text="#000000"` on the
same `<body>`, a separate finding this fixer does not touch, so neither book
becomes valid from this repair alone. Two books from one producer show that
the shape exists. They do not show how often it occurs.

**Measured after building (544 books, epubveri 0.26.0, 2026-10-14).** One
proposal on each of the 2 books, and all 12 findings cleared. The two books
enter `regression_audit`'s touched set at 24 errors before and 12 after; the
12 that stay are the `text` attributes. No regression at any severity, and
the whole-shelf plan digest gains exactly those two lines.

## RSC-005 — an NCX `navPoint` with no `id`

> **Written spec-first on 2026-10-14.** The finding shape below is what
> epubveri 0.27.0 reported on the shelf before any code existed.

**Finding.** `ncx.schema_violation`, kind `missing_attribute`, `params` =
`["navPoint", "id"]` (the element, then the one attribute it still needs), and
`element_path` names the element:

```
/ncx:ncx[1]/ncx:navMap[1]/ncx:navPoint[1]      params ["navPoint", "id"]
```

**The shape.** The NCX grammar requires an `id` on every `navPoint`. On the
shelf the shape is a hand-built NCX in which no `navPoint` has one:

```xml
<navPoint playOrder="1">
  <navLabel><text>Kapak</text></navLabel>
  <content src="Text/kapak.xhtml"/>
</navPoint>
```

**Fix** (`fix.ncx_navpoint_id`, ConfirmNeeded). Add `id="<element>-<n>"`
(`navPoint-1`, `navPoint-2`, …) to each reported element, right after its name
in the start tag. `n` is the element's 1-based position among same-named
elements in document order. If that value is already used in the NCX, it is
made unique with the suffix `-2`, `-3`, … as in `fix.ncx_duplicate_id`. Nothing
else in the file changes. One proposal per NCX.

**Why it is safe.** The element has no id, so nothing can refer to one; adding
it cannot break a reference. NCX ids are labels, not link targets (see the NCX
internal-consistency section), so the value chosen carries no meaning beyond
being valid and unique. Every value is an NCName and is checked against all
ids already in the file. This is the same argument `fix.ncx_duplicate_id`
rests on, where the value is also ours. ConfirmNeeded for the same reason: the
id is a value we chose, and the user sees it.

**When it declines.**

- **A finding that names more than `id`** in `params[1..]`. Another missing
  attribute is a different defect with no value we could supply.
- **An element that already has an `id`** (a stale finding), or a node whose
  path no finding names.
- **An NCX that does not parse**, or an element outside the NCX namespace.

**Measured (562 books, epubveri 0.27.0, before building).** 1 book, 22
findings, every `navPoint` in its NCX (*Algernon'a Çiçekler*, one of the 18
books added to the shelf on 2026-10-14). The same NCX also has every
`playOrder` at `1`, which `fix.ncx_play_order` already repairs. One book shows
that the shape exists. It does not show how often it occurs.

**Measured after building (562 books, epubveri 0.27.0, 2026-10-14).** One
proposal, on that book, and all 22 findings cleared. Errors after an
approve-all repair fall from 16,757 to 16,735 across the shelf. No regression
at any severity, and the whole-shelf plan digest gains exactly that one line.
The book is still not valid: its NCX names a `Text/kapak.xhtml` the book does
not contain, which is not this fixer's to repair.

## RSC-005 — a nested list sitting directly in its parent list

**Finding.** `opf.content_document.schema_violation`, kind
`element_not_allowed`, `params[0]` = `ul` or `ol` (`params[1..]` the
elements allowed there: `li` in EPUB 2; `li`, `script`, `template` in EPUB 3),
with an `element_path` naming the list. A `<ul>` or `<ol>` may hold only `<li>`
(and script-supporting elements), so a list that is its direct child is
invalid in both versions. The shape is what a converter writes for a nested
table of contents:

```html
<ul><li><a href="main-1.xhtml">Uyanış</a></li><ul><li>…</li></ul><li>…</li></ul>
→
<ul><li><a href="main-1.xhtml">Uyanış</a><ul><li>…</li></ul></li><li>…</li></ul>
```

**Fix** (`fix.nested_list_in_list`, ConfirmNeeded). For each reported list
whose parent is a `<ul>`/`<ol>` and whose nearest preceding sibling element is
an `<li>`, move that `<li>`'s end tag to just after the list. Nothing is
created and nothing is deleted: the end tag is the `<li>`'s own, byte for
byte, so the edit is a move of five bytes. A run of reported lists after one
`<li>` (`</li><ul>…</ul><ul>…</ul>`) all go into it, in order, by moving the
end tag past the last of them. A reported list inside another reported list
is handled the same way inside it. One proposal per document.

**Why it is one repair.** The list must become the child of an `<li>`, and
the only content-preserving way is to put it into an existing one. Of the
existing ones, the `<li>` immediately before it is the item it nests under:
that is what the indentation of a nested list means, and it is where every
reader that recovers from the error shows it. Wrapping it in a new `<li>`
instead would add an item (an empty marker in a `<ul>`, a number in an `<ol>`),
which changes what is shown.

**What the move does not change.** The elements and their order in the
document are the same before and after: only the point where the `<li>`
closes moves. The outer list's item count and an `<ol>`'s numbering count
`<li>` children only, and the moved element is a list. The user-agent rules
that indent and space nested lists (`ul ul`, `ol ul`, … in HTML §15) match the
list as a descendant of the outer list in both positions.

**The book's own CSS is checked, not assumed (styloria).** Author CSS could
tell the two positions apart: `li > ul`, `ul > ul`, `li ul`, `li:last-child`,
`:nth-child()`, `:empty`, `+` and `~`. So the document is parsed before and
after the move. Every style rule in every `.css` entry of the book and in the
document's `<style>` elements is matched with styloria against every element in
both trees, paired by document order. The document is declined if any
selector matches any element differently, or if any answer is `Unknown`
(`:hover`, `:has()`, `&`, …). A selector with a pseudo-element (`li::before`)
is matched without it, since the box it styles is generated by that element. A
selector the browser drops (an invalid selector list) styles nothing and is
skipped. Every sheet of the book is read, not only the ones the document links:
that can only decline more.

**When it declines.**

- **The list's preceding sibling element is not an `<li>`**, or the list is
  its parent's first child. The list would need an item we would have to
  invent.
- **Anything but whitespace between the `<li>` and the list**, or between
  lists of a run: text, a comment, a processing instruction. Where it would
  go is a choice.
- **The `<li>` has no end tag of its own** (`<li/>`).
- **The parent is not a `<ul>`/`<ol>`**, or the list, the parent or the `<li>`
  is not in the XHTML namespace. A list in a `<p>` (one case on the shelf) is
  a different defect.
- **A selector tells the positions apart, or answers `Unknown`** (above).
- **A stylesheet that does not decode losslessly**, one with `@namespace` (the
  prefixes would have to be resolved per sheet), or a style rule nested in
  another style rule: the whole document declines.
- The document does not parse.

**Measured (562 books, epubveri 0.31.0, 2026-10-22, before building).** 19
lists in 8 books, as epublift's note counted. 7 books are invalid only over
this. In 7 of them the lists are a converter's `main.xhtml` table of contents;
the 8th has one in each of two body documents. One more finding of the same
kind is a `<ul>` inside a `<p>`, which this fixer does not take. One book
has reported lists nested three deep (`ul[3]/ul[1]/ul[1]`). The CSS of the 7
TOC books has no combinator and no pseudo-class at all. The 8th book's has
combinators and `:first-child`/`:last-child`, none on `ul`, `ol` or `li`.

**Measured after building (562 books, epubveri 0.31.0, 2026-10-22).** The
whole-shelf A/B (`bump_ab`, against `cca571f`) moves exactly the 8 books: one
proposal per reported document (9), every one applied, none reverted, and
every reported list cleared: `schema_violation` falls by exactly 19 there.
The one left, in *Kadınlar*, is an `<img>` with no `alt`. No other book changes. After
`repair -y`, 4 of the 8 are valid; 3 more hold only `clear` attributes
(`htm.obsolete_attribute`), which epublift's EPUB 3 upgrade converts.

