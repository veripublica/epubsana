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
| `RSC-005` | `htm.epub2_dom.bare_text_in_body` | ConfirmNeeded | EPUB 2 text sits directly in `<body>` with no block-level element around it | [Wrap the text in a `<div>`, leaving whitespace alone](#rsc-005--bare-text-directly-in-body-epub-2) |
| `RSC-001` | `opf.manifest_item.missing_resource` | ConfirmNeeded | A manifest `<item>` declares a resource the container doesn't hold | [Drop the item, and every reference that named it](#rsc-001--dangling-manifest-item) |
| `OPF-049` | `opf.spine.itemref_idref_not_in_manifest` | ConfirmNeeded | A spine `<itemref>` names a manifest id that doesn't exist | [Drop the itemref](#opf-049--dangling-spine-itemref) |
| `OPF-034` / `RSC-005` | `opf.spine.duplicate_itemref` | ConfirmNeeded | The spine lists the same manifest item more than once | [Keep the first occurrence, drop the later ones](#opf-034--rsc-005--duplicate-spine-itemref) |
| `HTM-004` | `htm.doctype.epub3_obsolete_public_id` | AutoSafe | An EPUB 3 document's DOCTYPE carries an obsolete PUBLIC identifier | [Reduce it to `<!DOCTYPE html>`](#htm-004--obsolete-or-unrecognized-doctype) |
| `HTM-004` | `htm.doctype.epub2_unrecognized_public_id` | ConfirmNeeded | An EPUB 2 document's DOCTYPE isn't a recognized XHTML 1.1 / OEB identifier | [Canonicalize a malformed XHTML 1.1 id; decline a genuinely different DTD](#htm-004--obsolete-or-unrecognized-doctype) |
| `RSC-005` | `ncx.ids.duplicate_id` | ConfirmNeeded | Two or more NCX elements share an `id` | [Keep the first, rename later duplicates uniquely](#rsc-005--ncx-internal-consistency) |
| `RSC-005` | `ncx.play_order.duplicate` | ConfirmNeeded | Navigation elements repeat a `playOrder` value | [Renumber `playOrder` by document order](#rsc-005--ncx-internal-consistency) |

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
