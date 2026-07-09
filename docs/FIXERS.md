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

**A note on structural fixers.** Fixers that must locate an element (rather than
match a token) parse the document with `roxmltree` using `allow_dtd: true`, the
same option epubveri uses. NCX files and many XHTML documents declare a
`DOCTYPE`, which roxmltree's default parser rejects; matching epubveri's setting
means a structural fixer sees exactly the documents epubveri did. If a document
still won't parse, the fixer declines.

---

## RSC-016 — undeclared HTML entities

**Finding.** `htm.entity.undeclared`. An XHTML document references an HTML named
entity — `&nbsp;`, `&mdash;`, `&eacute;`, … — without a DTD that declares it.
epubveri reports the entity's name (in `params[0]`) and the file. Grouped per
file (a single book can have thousands).

**Fix** (`fix.html_entities`, AutoSafe). For each recognized entity, replace
every `&name;` occurrence in the file with the exact Unicode character that
entity denotes (from a curated Latin-1 + General-Punctuation + common-symbol
table). Example: `&nbsp;` → U+00A0, `&mdash;` → `—`.

**Why it's safe.** These are standard HTML named entities; substituting the
character they denote changes only the *encoding* of that character, not the
rendered content. The result no longer relies on an undeclared entity, so the
error clears.

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

## Adding a fixer

When you add a fixer, add a section here using the same shape — **Finding**,
**Fix** (with its `fix_id` and tier), **Why it's safe**, **When it declines** —
and a row in the [Summary](#summary) table. Then update the capability table in
[USAGE.md](./USAGE.md#what-epubsana-can-fix-today). Keep the safety argument
concrete: what exactly changes, and why that can't corrupt the book.
