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
| `RSC-005` | `ncx.ids.invalid_ncname` | ConfirmNeeded | An NCX `id` isn't a valid XML NCName | [Sanitize it to a valid, unique NCName](#rsc-005--invalid-ncx-id-ncname) |
| `RSC-005` | `opf.content_document.invalid_content_type_meta` | ConfirmNeeded | A legacy `<meta http-equiv="Content-Type">` has the wrong value | [Normalize to a single HTML5 `<meta charset="utf-8">`](#rsc-005--content-type-encoding-declaration) |
| `NCX-001` | *(none)* | ConfirmNeeded | The NCX `dtb:uid` disagrees with the package identifier | [Set `dtb:uid` to the package's unique identifier](#ncx-001--ncx-dtbuid-mismatch) |
| `RSC-005` | `opf.content_document.empty_title` | ConfirmNeeded | An XHTML `<title>` element is empty | [Fill it from the book's own TOC label, else its first heading](#rsc-005--empty-title) |
| `RSC-020` | `opf.manifest_item.unencoded_space_in_href` | AutoSafe | A manifest `href` contains a raw space | [Percent-encode the space as `%20`](#rsc-020--unencoded-space-in-a-manifest-href) |
| `OPF-014` | `opf.content_document.property_used_undeclared` | AutoSafe | A content document uses a feature its manifest item doesn't declare | [Add the token to that item's `properties`](#opf-014--undeclared-content-property) |
| `PKG-006` | *(none)* | AutoSafe | The `mimetype` entry is not first in the ZIP, as OCF requires | [Re-emit it first and stored, touching no content](#pkg-006--mimetype-is-not-the-first-entry) |
| `RSC-005` | `htm.epub2_dom.bare_text_in_body` | ConfirmNeeded | EPUB 2 text sits directly in `<body>` with no block-level element around it | [Wrap the text in a `<div>`, leaving whitespace alone](#rsc-005--bare-text-directly-in-body-epub-2) |
| `RSC-001` | `opf.manifest_item.missing_resource` | ConfirmNeeded | A manifest `<item>` declares a resource the container doesn't hold | [Drop the item, and every reference that named it](#rsc-001--dangling-manifest-item) |
| `OPF-049` | `opf.spine.itemref_idref_not_in_manifest` | ConfirmNeeded | A spine `<itemref>` names a manifest id that doesn't exist | [Drop the itemref](#opf-049--dangling-spine-itemref) |

**A note on structural fixers.** Fixers that must locate an element (rather than
match a token) parse the document with `roxmltree` using `allow_dtd: true`, the
same option epubveri uses. NCX files and many XHTML documents declare a
`DOCTYPE`, which roxmltree's default parser rejects; matching epubveri's setting
means a structural fixer sees exactly the documents epubveri did. If a document
still won't parse, the fixer declines.

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

## RSC-005 — bare text directly in `<body>` (EPUB 2)

**Finding.** `htm.epub2_dom.bare_text_in_body`. An EPUB 2 content document has
text sitting directly inside `<body>`, with no block-level element around it.
XHTML 1.1 requires `<body>` to contain block-level content, so this is invalid
there. (EPUB 3 is HTML5, where `<body>` accepts flow content directly — hence the
rule's EPUB-2 scope.) `params` is empty, so epubsana parses the document and
locates the text itself.

**Fix** (`fix.bare_text_in_body`, ConfirmNeeded). Wrap each run of bare text in a
`<div>`, grouped one proposal per document. The wrapper goes around the text's
**non-whitespace span only**: `"\n\n\n50\n"` becomes `"\n\n\n<div>50</div>\n"`,
so the document's existing line breaks and indentation are untouched.

**Why it's safe.** The text itself is never altered — not a character is added,
removed or re-ordered; a wrapper appears around it and nothing else in the
document is touched. `<div>` is chosen deliberately over `<p>`:

- It makes **no claim about what the text is.** In the real corpus this text is
  usually a chapter title or a stray paragraph a converter left behind; calling
  it a paragraph would be a guess about intent, and calling it a heading more so.
- It **renders where it already rendered.** A reading system already lays bare
  text out in an anonymous block, which is exactly what a `<div>` is; a `<p>`
  would add default margins and push the page around.

**When it declines.** If the document doesn't parse, or it has no `<body>`,
nothing is changed.

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

**Measured.** 2 books in the 171-book corpus, both the same shape: a conversion
left `cover-1.jpg`/`cover-2.jpg` declared beside the real, present cover
(`id="cover"` → `cover.jpeg`, which is what `<meta name="cover">` actually names).
On this corpus neither guard fires — the dangling items are images, so nothing in
the spine references them, and they are not the declared cover. Grepping every
content document, the NCX and the OPF confirms the manifest entry itself is the
**only** thing in either book that mentions them. The guards above are therefore
argued rather than corpus-tested, and are covered by unit tests instead.

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
