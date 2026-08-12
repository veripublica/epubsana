# epubsana — Usage Guide

**epubsana repairs the EPUB defects [epubveri](https://github.com/veripublica/epubveri)
detects.** It turns the safely-fixable findings into edits you approve one at a
time, applies the approved ones, and reports exactly what changed. It never
guesses, and it preserves — byte-for-byte — everything it doesn't touch.

- **In scope:** *repair* — surgical, content-preserving fixes that clear
  validation errors while keeping the book otherwise identical.
- **Out of scope:** *modernization* (upgrading EPUB 2 → 3.3, rebuilding the
  table of contents, fetching metadata, archiving). That is
  [epublift](https://github.com/ePubLift/epublift)'s job. epubsana deliberately
  serves publishers who still ship EPUB 2 for older readers, so it fixes a book
  *in place* without changing its version.

---

## Table of contents

- [Install](#install)
- [Quick start](#quick-start)
- [How it works](#how-it-works)
- [CLI reference](#cli-reference)
- [The interactive workflow](#the-interactive-workflow)
- [The repair report](#the-repair-report)
- [Exit codes](#exit-codes)
- [What epubsana can fix today](#what-epubsana-can-fix-today)
- [Safety guarantees](#safety-guarantees)
- [Reference standard](#reference-standard)
- [Using epubsana as a Rust library](#using-epubsana-as-a-rust-library)
- [Known limitations](#known-limitations)

---

## Install

epubsana is pure Rust with no C dependencies.

**The CLI** — from [crates.io](https://crates.io/crates/epubsana):

```sh
cargo install epubsana
```

**In the browser** — no install at all: repair an EPUB with the
[in-browser demo](https://veripublica.github.io/epubsana/) (your file never
leaves the page). For a JS project, add the WASM bindings from npm:

```sh
npm install @veripublica/epubsana-wasm
```

**From source** (optional):

```sh
git clone https://github.com/veripublica/epubsana
cd epubsana
cargo install --path .
```

---

## Quick start

```sh
# 1. See what would be fixed — changes nothing:
epubsana -i book.epub --dry-run

# 2. Repair interactively, approving each fix (writes book_fixed.epub):
epubsana -i book.epub

# 3. Apply every proposed fix without prompting, to a chosen path:
epubsana -i book.epub --yes -o repaired.epub
```

`-i` is the only input form — a positional path is a usage error, so a typo can
never be mistaken for a filename. The original is never modified in place; a
repaired copy is written to `<name>_fixed.epub` (or your `-o`) only if at least
one fix was applied, and an existing output file is never silently replaced
(pass `-f` to allow it).

---

## How it works

epubsana follows a strict five-step contract, the same in every frontend (this
CLI, the in-browser WASM demo, and library consumers such as epublift):

1. **Load** the EPUB into a fidelity-preserving in-memory container. Untouched
   entries round-trip byte-for-byte.
2. **Detect** — run epubveri over the book to get the findings (each with an
   epubcheck-compatible ID, a stable `rule` sub-code, and an exact position).
3. **Propose** — for each finding a fixer can safely handle, build a
   `ProposedFix`: a description, the reason it is safe, and a preview of the
   exact edits. Findings epubsana can't safely fix are left alone.
4. **Confirm** — you decide, per fix, whether to apply it. Nothing mutates
   without your approval (subject to the [policy](#cli-reference) you choose).
5. **Report** — the run ends with a record of what became of every proposed fix
   (applied, skipped, or — in a dry run — merely proposed), the fatal and error
   counts before vs. after, and whether the goal was met.

---

## CLI reference

epubsana conforms to the **[veripublica CLI convention v0.4](https://github.com/veripublica/conventions/blob/main/CLI.md)**,
so its flags, output naming, and exit codes match the other veripublica tools.

```
epubsana -i <PATH> [OPTIONS]
```

| Option | Description |
| --- | --- |
| `-i`, `--input <PATH>` | The input. The only input form; positional paths are not accepted. |
| `-o`, `--output <PATH>` | Where to write the output. Default: `<input-stem>_fixed.epub`, beside the input. Must not be the input. |
| `-f`, `--force` | Permit replacing existing output files. Never lifts the output-equals-input refusal. |
| `--format <FORMAT>` | Report format: `human` (the default) or `json` — the shared machine envelope. |
| `--dry-run` | Report what would happen; change nothing on disk. |
| `-y`, `--yes` | Assume "yes" for every prompt; run non-interactively. Not permission to overwrite files — that is `-f`. |
| `--auto-safe` | Apply the provably-safe fixes without asking; still prompt for the rest. |
| `--apply <LIST>` | Apply exactly the listed fixes, skip the rest, ask nothing. Selectors are 1-based plan indices and/or fix ids, comma-separated. |
| `--goal <valid\|openable>` | How far to repair. Default: `valid`. See [Exit codes](#exit-codes). |
| `-v`, `--verbose` | Show each fix's rationale (why it's safe). |
| `-V`, `--version` | Print `epubsana <version>` and exit `0`. |
| `-h`, `--help` | Print help and exit `0`. |

**Behaviour of the four modes**

- **Default (no flag):** prompt for *every* proposed fix (`[y/N]`).
- **`--auto-safe`:** apply fixes tiered *AutoSafe* automatically; prompt for
  *ConfirmNeeded* fixes. See [tiers](#the-interactive-workflow).
- **`-y`/`--yes`:** approve everything, no prompts. Good for batch/CI use — but
  read [Safety guarantees](#safety-guarantees) first, and prefer `--dry-run` to
  preview.
- **`--apply <LIST>`:** approve exactly what you name and nothing else. This is
  the mode for a plugin or a script that has already asked a human — see
  [Applying only some of the fixes](#applying-only-some-of-the-fixes---apply).

### Applying only some of the fixes (`--apply`)

Sometimes you want three of the eight fixes epubsana proposes, not all eight and
not one at a time in a terminal. `--apply` does exactly that: **you tell it which
fixes to make, it makes those and skips the rest, and it asks you nothing.**

This is also how a plugin or a script drives epubsana: it shows the user a list
in its own window, the user ticks some boxes, and the plugin passes the ticked
ones to `--apply`.

#### The whole thing in three steps

**Step 1 — ask what it would do.** `--dry-run` changes nothing at all. It just
prints the plan, numbered:

```console
$ epubsana -i book.epub --dry-run
book.epub: 0 fatal(s), 90 error(s) before repair

— proposed fixes (dry run: nothing was changed) —
[1] WOULD APPLY Make 1 invalid NCX id a valid XML NCName in toc.ncx
    - rename NCX id "59a835d2-f837…" → "id_59a835d2-f837…"
[2] WOULD APPLY Drop 25 legacy <a name> attributes in chapter-04.html
    - drop 25 obsolete name= attributes
[3] WOULD APPLY Drop 19 legacy <a name> attributes in chapter-05.html
    - drop 19 obsolete name= attributes

0 fatal(s), 90 error(s) → 0 fatal(s), 90 error(s)
would write book_fixed.epub
goal 'valid': NOT MET
```

The first line is the state of the book before anything happens; the numbers in
brackets are what `--apply` wants. Add `-v` if you also want each fix to explain
*why* it is safe.

(That first line is not printed with `--format json`, so the JSON output stays a
single object you can parse directly.)

**Step 2 — decide.** Say you want the NCX rename and the second file's cleanup,
but not the first file's. That is fixes **1** and **3**.

**Step 3 — apply just those.**

```console
$ epubsana -i book.epub --apply 1,3
book.epub: 0 fatal(s), 90 error(s) before repair

— repair report —
[1] APPLIED Make 1 invalid NCX id a valid XML NCName in toc.ncx
[2] SKIPPED Drop 25 legacy <a name> attributes in chapter-04.html
[3] APPLIED Drop 19 legacy <a name> attributes in chapter-05.html

0 fatal(s), 90 error(s) → 0 fatal(s), 70 error(s)
wrote book_fixed.epub
goal 'valid': NOT MET
```

Every fix is still listed, so you can see what you turned down. `book.epub` is
untouched; the repaired copy is `book_fixed.epub`.

#### What the words mean

| Word | Meaning |
|---|---|
| `WOULD APPLY` | Only appears under `--dry-run`. Nothing happened. |
| `APPLIED` | This fix was made in the output file. |
| `SKIPPED` | This fix was proposed and *not* made — you didn't select it, or you answered `n`. |

#### Ways to name a fix

| You write | It means |
|---|---|
| `--apply 2` | Just fix number 2 from the plan. |
| `--apply 1,3,7` | Fixes 1, 3 and 7. Commas, no spaces needed. |
| `--apply "1, 3, 7"` | The same — spaces around commas are fine if you quote it. |
| `--apply fix.html_entities` | *Every* fix produced by that fixer, however many files it touches. |
| `--apply 2,fix.html_entities` | Mix freely: fix 2, plus all of that fixer's fixes. |

**A number is always a position in the plan, never the name of a fixer.** Fixer
names always start with `fix.` — you can see each one in the JSON output
(`data.fix_id`) or in [the fixer catalogue](./FIXERS.md).

There is no `--apply all`. To apply everything, use `-y`.

#### Doing it from a program

Run the dry run with `--format json` and you get one JSON object. Each proposed
fix is an item, and each item carries the number you feed back to `--apply`:

```json
{
  "type": "fix",
  "outcome": "proposed",
  "code": "RSC-005",
  "rule": "ncx.ids.invalid_ncname",
  "severity": "error",
  "location": "toc.ncx",
  "message": "Make 1 invalid NCX id a valid XML NCName in toc.ncx",
  "data": {
    "index": 1,
    "fix_id": "fix.ncx_ncnames",
    "tier": "confirm_needed",
    "changes": [
      {
        "path": "toc.ncx",
        "note": "rename NCX id \"59a835d2-f837…\" → \"id_59a835d2-f837…\""
      }
    ]
  }
}
```

The fields you will actually use:

- **`data.index`** — the selector. Pass it to `--apply`.
- **`message`** — a one-line description to show the user.
- **`data.changes`** — the individual edits. Each one is an object with a
  **`path`** (which file inside the EPUB it touches) and a **`note`** (what it
  does there, in words).
- **`data.tier`** — `auto_safe` (provably safe) or `confirm_needed` (a judgement
  call). Useful if you want to pre-tick some boxes and not others.
- **`severity`** — the severity of the *defect being repaired*, straight from
  epubveri. It is never a judgement about the fix.
- **`location`** — the file inside the EPUB, when the fix touches only one.

#### Knowing which files changed

This is the question a plugin actually needs answered: *after epubsana runs, which
files do I copy back?*

You do not have to work it out. Every edit names its file, so the set of files a
run touches is just the paths in `data.changes`:

```python
touched = {c["path"] for it in items for c in it["data"]["changes"]}
# {'content.opf', 'toc.ncx', 'werther_split_004.html', …}
```

Two things follow that are worth knowing:

- **A fix that spans files produces one entry per file.** Renaming an invalid id,
  for example, rewrites the document holding it, every document linking to it,
  and the NCX — that is one fix with several `changes` entries, one per file.
- **You can ask before applying.** Those paths are in the `--dry-run` output too,
  so you can tell a user exactly which files a fix would touch without touching
  anything.

**One exception, and it is the only one.** The packaging fix (`PKG-006`,
`fix.mimetype_packaging`) reports `"path": "mimetype"` — but that file's *content*
does not change. What changes is where it sits in the ZIP and whether it is
compressed. A plugin that copies changed files back cannot reproduce that fix,
because in an editor's terms it is not a file edit at all. If you see it, either
re-save the container yourself or use epubsana's own output file for that book.

**You do not need to diff the repaired EPUB against the original.** epubsana
copies every entry it did not touch through byte-for-byte — same bytes, same
compression method, same timestamps — so an untouched file is identical, and
`data.changes` already tells you which ones those aren't.

A complete Python round tripA complete Python round trip, which is roughly what a Sigil or calibre plugin
does:

```python
import json, subprocess

# 1. Ask for the plan. Nothing is modified by this call.
plan = json.loads(subprocess.run(
    ["epubsana", "-i", "book.epub", "--dry-run", "--format", "json"],
    capture_output=True, text=True, check=True).stdout)

items = plan["inputs"][0]["items"]
for it in items:
    print(it["data"]["index"], it["message"])

# 2. Let the user choose. Here: everything that is provably safe.
chosen = [str(it["data"]["index"])
          for it in items if it["data"]["tier"] == "auto_safe"]

# 3. Apply exactly those. Skip the call entirely if the user picked nothing.
if chosen:
    subprocess.run(
        ["epubsana", "-i", "book.epub", "-o", "fixed.epub",
         "--apply", ",".join(chosen), "--format", "json"],
        check=True)
```

The same thing in a shell, if you have `jq`:

```console
$ SEL=$(epubsana -i book.epub --dry-run --format json \
        | jq -r '[.inputs[0].items[] | select(.data.tier=="auto_safe")
                  | .data.index] | join(",")')
$ epubsana -i book.epub -o fixed.epub --apply "$SEL"
```

#### Why two runs is safe

You are selecting from one run and applying in another, so the numbers only mean
something if both runs plan the same way. **They do.** Planning is
deterministic: the same input file plus the same epubveri version always produces
the same fixes in the same order.

What breaks that, and what to do:

- **You edited the book between the two calls** → run `--dry-run` again and
  re-pick. The numbers may have moved.
- **epubsana or epubveri was updated between the two calls** → same answer.
- **You are applying to a different file than you planned on** → the numbers are
  meaningless. Plan and apply against the same input.

If a number no longer exists, epubsana tells you instead of guessing — see below.

#### When something goes wrong

Each of these stops the run **before writing anything**. Your input file is never
modified in any case, and no output file is left half-done.

**A number that isn't in the plan** (usually a stale list, or a typo):

```console
$ epubsana -i book.epub --apply 1,99
book.epub: 0 fatal(s), 90 error(s) before repair
error: --apply selector(s) matched no proposed fix: 99. This run planned 48
fix(es); re-run with --dry-run to see the current plan. Nothing was written.
```

Note that fix 1 was **not** applied either. Applying the half of your list that
happened to match would leave you believing something about the book that isn't
true, so the whole run is refused.

**A fixer name that proposed nothing this time** — same error. A fixer only
appears in the plan when the book has the defect it repairs, so
`--apply fix.html_entities` on a book with no entity problems is an error, not a
no-op. Check the plan first.

**Combining `--apply` with a flag that decides differently:**

```console
$ epubsana -i book.epub --apply 1 --dry-run
error: --apply and --dry-run contradict each other: --apply already answers
every prompt, and only for the fixes you listed
```

The same for `-y`/`--yes` and `--auto-safe`. `--apply` is itself a complete set
of answers, so epubsana refuses rather than quietly picking a winner and editing
a book on a guess.

**An empty list:**

```console
$ epubsana -i book.epub --apply ""
error: --apply was given no selectors; to apply nothing, simply do not run the
repair, and to apply everything use --yes
```

**The output file already exists:**

```console
$ epubsana -i book.epub --apply 1
error: 'book_fixed.epub' exists; use -f to replace it
```

Add `-f`, or choose another path with `-o`. This is not specific to `--apply`.

**Nothing was proposed at all.** Then there is nothing to select, and any
selector is an error. A book epubsana cannot help with prints `No fixes to
propose.` under `--dry-run` — that is the signal to stop, not to try `--apply`.

#### Three things you can rely on

1. **Your original is never modified.** Repairs go to a separate file, always.
2. **Only what you named is applied.** Everything else is reported as `SKIPPED`,
   so the report is a full account of what was offered and what you took.
3. **A partial match is never applied.** Either every selector matches and the
   run proceeds, or nothing is written at all.

A prompt epubsana cannot ask is a decision it cannot obtain: when stdin is not a
terminal and fixes would need approval, it **stops** (exit `2`) and names the
flag that would let it proceed, rather than silently assuming "no" and returning
an exit code that looks like an ordinary result.

**`--goal`** does not yet change which fixers run (both goals propose the same
fixes; the distinction will gate fixer selection as the registry grows) — but it
**does** decide what counts as success, and therefore the exit code. See below.

---

## The interactive workflow

In the default and `--auto-safe` modes, epubsana prints each proposed fix and
asks before applying it:

```
[ConfirmNeeded] Make 55 invalid NCX ids a valid XML NCName in toc.ncx
    - rename NCX id "51100e1e-b21d-4d41-…" → "id_51100e1e-b21d-4d41-…"
    - rename NCX id "36d9b249-ecd7-4ebe-…" → "id_36d9b249-ecd7-4ebe-…"
    …
  Apply this fix? [y/N]
```

- Type `y` (or `Y`) to apply; anything else — including just Enter — skips it.
- The `[Tier]` prefix tells you how much intervention the fix needs:
  - **`AutoSafe`** — exactly one correct, content-preserving fix; safe to apply
    unattended. `--auto-safe` applies these without asking.
  - **`ConfirmNeeded`** — a good fix that makes a visible change (e.g. renaming
    an id, rewriting an encoding declaration); you should look before approving.
- The indented lines are the **preview**: the exact edits this fix would make.

---

## The repair report

Every run (except `--dry-run`) ends with a report:

```
— repair report —
APPLIED Map 1 undeclared HTML entity (657×) to characters in OEBPS/Text/bolum2.xhtml (nbsp)
    - replace &nbsp; → ' ' (657×)
SKIPPED Make 55 invalid NCX ids a valid XML NCName in toc.ncx
    - rename NCX id "51100e1e-…" → "id_51100e1e-…"

774 fatal(s), 5 error(s) → 0 fatal(s), 4 error(s)
wrote book_fixed.epub
goal 'valid': NOT MET
```

- Each fix line says what became of it: **APPLIED**, **SKIPPED** (you declined),
  or **WOULD APPLY** (a `--dry-run`). The indented lines are its concrete edits.
- **N fatal(s), N error(s) → …** is epubveri's own count before repair vs. after
  — the book is re-validated at the end, so this is an independent check, not a
  claim.
- **Fatals are counted apart from errors**, exactly as epubveri reports them. A
  fatal is a defect that stops the book from being processed at all (an
  unreadable ZIP, a missing `container.xml`, XHTML that is not well-formed, an
  unterminated entity reference). A book whose defects are all fatal has *zero
  errors* and is not remotely valid — so the fatal count is stated first, and
  always.
- **wrote …** appears only if at least one fix was applied.
- **goal '…': MET / NOT MET** is the line the exit code mirrors.

---

## Exit codes

Per the [convention](https://github.com/veripublica/conventions/blob/main/CLI.md#6-exit-codes),
a transformer's `0` means *the run's goal was met* — and epubsana has two goals:

| Code | Meaning |
| --- | --- |
| `0` | The run's goal was met. With `--goal valid` (the default): no fatal- and no error-severity findings remain — the book is valid. With `--goal openable`: no fatal-severity findings remain — the book opens. |
| `1` | The goal was not met: fixes were declined, or defects epubsana cannot fix remain. |
| `2` | epubsana could not run: a usage error, an unreadable EPUB, `-o` equal to the input, an existing output file without `-f`, an unanswerable prompt, or an I/O failure. |

The default goal is the *verifier's* threshold, so `epubsana -i book.epub -y &&
echo "valid"` means what `epubveri -i book.epub && echo "valid"` means — the two
tools agree by construction.

`--goal openable` is the explicitly-requested **lesser** goal the convention
allows: the e-reader / fix-on-import bar. Under it, **exit `0` can coexist with
errors in the report** — the book opens, which is what the invocation asked. The
errors are still reported; they simply do not move the exit code. The goal is
always printed (and carried in `--format json`'s `summary.goal`), so a `0` is
never read without the bar it was measured against.

---

## Machine output (`--format json`)

`--format json` emits the shared veripublica envelope
([FORMATS.md](https://github.com/veripublica/conventions/blob/main/FORMATS.md)) —
exactly one JSON object on stdout, the same shape epubveri emits, so one parser
reads both:

```json
{
  "tool": "epubsana",
  "tool_version": "0.3.1",
  "convention": "0.4",
  "status": "problems",
  "inputs": [
    {
      "path": "book.epub",
      "status": "problems",
      "output": "book_fixed.epub",
      "summary": {
        "fatals_before": 774, "fatals_after": 0,
        "errors_before": 5, "errors_after": 4,
        "applied": 2, "skipped": 0, "goal": "valid"
      },
      "items": [
        {
          "type": "fix",
          "outcome": "applied",
          "code": "RSC-016",
          "rule": "htm.entity.undeclared",
          "severity": "fatal",
          "location": "OEBPS/Text/bolum2.xhtml",
          "message": "Map 1 undeclared HTML entity (657×) to characters …",
          "data": {
            "index": 1, "fix_id": "fix.html_entities", "tier": "auto_safe",
            "changes": [ { "path": "OEBPS/ch01.xhtml", "note": "…" } ]
          }
        }
      ]
    }
  ]
}
```

Two fields carry epubsana's half of the contract:

- **`outcome`** — `applied`, `skipped`, or `proposed` — is on **every** fix item.
  A confirm-each-step run routinely applies one fix and declines the next; a
  report that cannot say which is not a report of what changed. Under
  `--dry-run` every item is `"proposed"` (and `dry_run: true` is a summary of
  that, never a contradiction of it).
- **`severity`** is **inherited** from the finding the fix addresses, verbatim
  from epubveri — it describes the *defect*, never epubsana's opinion of its own
  fix. How much judgement the fix needs is a different axis, and lives in
  `data.tier`.

A usage error produces **no envelope**: a short message on stderr and exit `2`.

---

## What epubsana can fix today

Each fixer targets a specific epubveri finding — by epubcheck ID and, where
available, the stable `rule` sub-code — and only proposes an edit when a safe,
content-preserving one exists. For exactly *how* each fix is made, why it's
safe, and when epubsana declines, see the **[fix catalogue](./FIXERS.md)**.

| epubcheck ID | rule sub-code | Tier | What it does |
| --- | --- | --- | --- |
| `RSC-016` | `htm.entity.undeclared` | AutoSafe | Replaces undeclared HTML named entities (`&nbsp;`, `&mdash;`, `&eacute;`, …) used in XHTML without a DTD with the exact character each denotes. Entities it doesn't recognize are left untouched. |
| `RSC-016` | `htm.entity.missing_semicolon` | AutoSafe | Repairs a named entity reference missing its closing `;` (`&nbsp`), which is not well-formed XML. A recognized name becomes the character it denotes; an XML-predefined one (`&amp` …), whose character is the bare delimiter, is closed with `;` instead. Only the unterminated occurrences are touched — a correct `&nbsp;` and a longer entity are left alone; an unrecognized name is declined. |
| `RSC-005` | `ncx.ids.invalid_ncname` | ConfirmNeeded | Makes an invalid NCX `id` a valid XML NCName (e.g. a digit-leading UUID `51100e1e-…` → `id_51100e1e-…`, or a brace-wrapped GUID `{0F57…}` → `id_0F57…`), keeping it unique. Only rewrites an `id` whose attribute is unambiguous. |
| `RSC-005` | `opf.content_document.invalid_content_type_meta` | ConfirmNeeded | Normalizes a content document's encoding declaration to the EPUB 3.3 / HTML5 form: collapses every legacy `<meta http-equiv="Content-Type">` (and any duplicate) into a single `<meta charset="utf-8"/>`. Declines if the document declares a non-UTF-8 charset. |
| `NCX-001` | *(none)* | ConfirmNeeded | Sets the NCX `dtb:uid` to the package's unique identifier (the `dc:identifier` the OPF `unique-identifier` points at), so the two agree. |
| `RSC-005` | `opf.content_document.empty_title` | ConfirmNeeded | Fills an empty `<title></title>` with text **from the book itself**: the label its table of contents gives that document, or failing that the document's own first heading. Declines when the book names the document nowhere — it never invents a title, and never falls back to the book's own `dc:title`. |
| `RSC-020` | `opf.manifest_item.unencoded_space_in_href` | AutoSafe | Percent-encodes a raw space in a manifest `href` (`ch 1.xhtml` → `ch%201.xhtml`). The file keeps its name; only the URL is spelled legally. |
| `OPF-014` | `opf.content_document.property_used_undeclared` | AutoSafe | Adds the property a content document demonstrably uses (`scripted`, `svg`, `remote-resources`, `switch`) to its manifest item's `properties`. The document itself is not touched — the manifest is made to tell the truth about it. |
| `PKG-006` | *(none)* | AutoSafe | Moves the `mimetype` entry to the front of the ZIP, stored uncompressed, as OCF requires. Changes no content at all — not one byte of any entry, `mimetype` included; only where it sits and how it's compressed. Declines if there is no `mimetype` entry to move. |
| `RSC-005` | `opf.content_document.schema_violation` (non-block content in `body` / `blockquote`) | ConfirmNeeded | Wraps EPUB 2 text **and inline elements** (`<a>`, `<br>`, `<img>`, …) sitting where the grammar requires block content — inside `<body>` or `<blockquote>` — in a `<div>`. Each run is wrapped whole, so a line that rendered as one block still does. Nothing is altered and the whitespace around it stays put. `<div>` rather than `<p>`: it claims nothing about what the content is, and matches the anonymous block it already renders as. Containers wanting a specific child (`<ol>` an `<li>`, `<head>` a `<title>`) are declined, as is an element XHTML 1.1 does not have at all (`<figure>`, `<section>`) — wrapping one would move its violation rather than clear it. |
| `RSC-001` | `opf.manifest_item.missing_resource` | ConfirmNeeded | Drops a manifest `<item>` declaring a resource the container doesn't hold — **and, in the same approval, every reference that named it**: the spine `<itemref>`s it would orphan, and a legacy `<meta name="cover">` pointing at it. Nothing readable is lost, because the resource was already gone. Declines if the deletions would empty the `<spine>`. |
| `OPF-049` | `opf.spine.itemref_idref_not_in_manifest` | ConfirmNeeded | Drops a spine `<itemref>` naming a manifest id that doesn't exist — a position no reading system can render, and one nothing in the book says how to repair. Every other entry keeps its place. Declines if it would empty the `<spine>`. |
| `OPF-034` (EPUB 2) / `RSC-005` (EPUB 3) | `opf.spine.duplicate_itemref` | ConfirmNeeded | Keeps the first spine `<itemref>` for a manifest item and drops the later repeats, so a chapter stops appearing twice in the reading order. The kept entry is where the document actually belongs, so nothing moves. Declines when the entries disagree on `linear` (in the reading order *and* reachable out-of-line is deliberate), or when a repeat carries an `id` a `<meta refines>` points at. |
| `HTM-004` | `htm.doctype.epub3_obsolete_public_id` | AutoSafe | An EPUB 3 (HTML5) document may carry only `<!DOCTYPE html>`; any PUBLIC identifier is obsolete, so the DOCTYPE is reduced to that. A doctype declares no content, so nothing a reader sees changes. Declines a DOCTYPE with an internal subset (`[ … ]`), whose entity declarations HTML5 can't carry. |
| `HTM-004` | `htm.doctype.epub2_unrecognized_public_id` | ConfirmNeeded | Canonicalizes an EPUB 2 DOCTYPE whose identifier is a **malformed XHTML 1.1** id (names 1.1, or the `xhtml11.dtd`, but mistypes the exact string) to the recognized form. Declines a document that declares a genuinely different DTD (XHTML 1.0, a bare `<!DOCTYPE html>`, OEB): relabeling it to 1.1 would assert a content model epubsana can't verify and risks trading the finding for content-model errors. |
| `RSC-005` | `ncx.ids.duplicate_id` | ConfirmNeeded | Two or more NCX elements share an `id`. Keeps the first and renames each later duplicate to a unique value. NCX ids aren't referenced by IDREF anywhere in an EPUB, so no reference is rewritten. |
| `RSC-005` | `ncx.play_order.duplicate`, `…target_mismatch`, `…gap` | ConfirmNeeded | The NCX's `playOrder` values are inconsistent — repeated across different targets, disagreeing about one target, or leaving a gap. All three interlock, so every value is reassigned the way the format defines: 1-based, dense, in document order, with elements naming the same target sharing the number that target was first given. `playOrder` is only a hint; the real reading order (the spine) is untouched. An NCX that will not parse is declined. |
| `RSC-007` | `opf.guide.reference_missing_resource` | ConfirmNeeded | Drops an EPUB 2 `<guide>` reference whose `href` resolves to no resource in the container — a landmark pointing at a hole. If that leaves the `<guide>` empty (invalid, and the element is optional), the `<guide>` is dropped too. Matches on the reported `href`; paths aren't re-resolved. |
| `RSC-017` | `opf.guide.duplicate_reference` | ConfirmNeeded | Two or more `<guide>` references share the same `type` **and** `href`. Keeps the first and drops the redundant repeats. References with the same `type` but different `href` (e.g. several `type="text"`) are not duplicates and are left alone. |
| `RSC-012` | `opf.guide.reference_fragment_not_defined` | ConfirmNeeded | A `<guide>` reference points into a document that **exists**, at a `#fragment` that document doesn't define — typically an anchor left behind by a conversion. The fragment is dropped and the path kept. This changes no behaviour: a fragment resolving to no `id` already opens that document at the top, exactly where the fragment-less href lands, so the edit writes down what the book already does. The landmark's target document is untouched, and the fragment is never repointed at some other `id` — that would be a guess. Declines when dropping the fragment would make the reference identical in `type` and `href` to another one in the same guide, which would trade this finding for a duplicate-reference one. |
| `RSC-005` | `htm.obsolete_attribute` (`name` on `<a>`) | AutoSafe | Drops the legacy `<a name="x">` attribute where the element already carries `id="x"` — the anchor is declared the modern way too, so every `#fragment` pointing at it still resolves and nothing is lost. An `<a name>` with **no** `id`, or with a *different* `id`, is left alone (renaming could produce an invalid or duplicate id; dropping would break the fragment). Other obsolete attributes in the same family, such as `<br clear>`, are left alone too — presentational intent has no single markup equivalent. |
| `RSC-005` | `opf.content_document.schema_violation` (empty `lang`/`xml:lang`) | ConfirmNeeded | Deletes an empty `lang=""` / `xml:lang=""`, which EPUB 2's grammar does not allow. Not a no-op, which is why you are asked: the element declared "undetermined" and will now inherit its parent's language, and reading systems use that for hyphenation, text-to-speech and font selection. A *malformed* tag (`en_US`) is left alone — repairing it would mean guessing the language. |

| `RSC-005` | `opf.content_document.schema_violation` (`params[0] == "id"`) | ConfirmNeeded | Renames a content-document `id` that is not a valid XML NCName (on real books: one starting with a digit) to the nearest valid, unique name — **and moves every reference with it**: fragments in the document, links from other documents, the NCX. References are resolved against the referring file's own directory rather than replaced globally, so a link meaning another document's identically-named id is left alone. Any occurrence it cannot classify — a fragment in a stylesheet selector, in script, or in prose — makes it decline that id rather than guess. |
| `RSC-005` | `opf.content_document.duplicate_id` | ConfirmNeeded | Two or more elements in one document share an `id`, which XML forbids. The first occurrence keeps it and each later one is renamed uniquely. No reference is rewritten and none needs to be: a `#fragment` already resolves to the first element carrying the id, so keeping the first leaves every link pointing exactly where it pointed. |
| `RSC-005` | `opf.package.schema_violation` | AutoSafe | Deletes an EPUB 3 attribute sitting on an EPUB 2 package document — but only after verifying, in that book, that it says nothing the book does not already say: a `properties="cover-image"` whose cover is also declared by `<meta name="cover">` on that same item, or a `page-progression-direction="ltr"`, which is the default everywhere. Any other `properties` token, a cover with no legacy declaration, or an `rtl` reading direction carries real information EPUB 2 cannot express, and is left alone. |
| `RSC-005` | `htm.epub2_dom.nested_anchor` | ConfirmNeeded | An `<a>` cannot contain another `<a>`. Where the outer one carries no `href` it is not a link but an **anchor target** — the legacy way of naming a position — so it is unwrapped and its `id` moves to its single child, and the fragment still resolves at the same place on the page. An outer anchor that is a real link, or that carries any attribute besides `id`, is left alone. |
| `RSC-007` | `opf.content_document.reference_missing_resource` | ConfirmNeeded | A link whose path no longer resolves but whose target is still in the book under the same name — a book restructured after it was written (`../Text/notes.xhtml#a8` where the file now sits beside the referring document). The path is repointed at the one container entry carrying that name, relative to the referring document, and the fragment is carried across. Declines when the name matches nothing or several entries, when the fragment is not in the chosen target (that would trade one error for a broken link), and for external URLs, scheme-less hostnames and placeholder junk. |
| `OPF-030` / `RSC-005` | `opf.package.unique_identifier_unresolved`, `opf.package.opf_identifier_not_empty` | ConfirmNeeded | The package says which identifier is canonical and that declaration lands on nothing usable — either no `<dc:identifier>` carries the named id, or the one that does is empty. The declared id is attached to the book's **single** real identifier and any leftover empty element is dropped; the NCX `dtb:uid` is synced in the same edit, since this repair is what first makes that comparison possible. Nothing is invented: the value was already in the book and the id already in the package. Declines when the book carries two candidate identifiers — choosing between a UUID and an ISBN is an editorial decision — or none at all. |
| `OPF-054` | *(none)* | ConfirmNeeded | Drops a `<dc:date>` with no content: it states no date, and `dc:date` is optional. A malformed but non-empty date (`March 2019`) carries a date the author wrote and is left exactly as it is — deciding which characters are stray would be a guess. Note `OPF-054` is EPUB 2 only; on EPUB 3 the same condition is a warning that never moves the validity line. |
| `OPF-072` | `opf.metadata.empty_element` | ConfirmNeeded | Drops an empty **optional** Dublin Core element (`dc:coverage`, `dc:source`, `dc:rights`, `dc:relation`, `dc:subject`, `dc:description`) from an EPUB 2 package: it states nothing, and its absence is valid. Never drops `dc:title`, `dc:identifier` or `dc:language` — deleting an empty *required* element would trade "empty" for "missing" — leaves `dc:date` to the fixer above, and declines an element a `<meta refines="#id">` points at, so no refinement is orphaned. **Usage severity: this clears report noise, not a validity failure.** |
| `OPF-090` | `opf.manifest_item.non_preferred_media_type` | ConfirmNeeded | Renames a manifest item's `media-type` from a superseded Core Media Type name to the current one for the same format: `application/vnd.ms-opentype` → `font/otf`, `application/x-font-ttf` → `font/ttf`, `application/font-woff` → `font/woff`, `application/ecmascript` and `text/javascript` → `application/javascript`. Renames the declaration only; it asserts nothing new about the file. **`application/font-sfnt` is declined** — SFNT is the container TrueType and OpenType share, so the name cannot say which the file is. **Usage severity.** |

Findings not in this table — arbitrary schema violations, and anything requiring
content epubsana would have to invent — are reported by epubveri but **left
untouched**. epubsana never guesses.

A dangling link is repaired only in the one case where nothing has to be
guessed: the file it names is still in the book. A link to something genuinely
absent stays as it is, and so does one whose target cannot be identified without
choosing between candidates.

Note the shape of the two dangling-reference fixers: they only ever delete a
pointer to something that **isn't there**, which is why deleting loses nothing.
A reference to a resource that *does* exist is never dropped to silence a
finding — that would be destroying content to make a validator happy.

The dangling-fragment fixer is the other side of that same rule. There the
target document *does* exist, so the reference is real and only the position
inside it is broken — which is exactly why that fixer drops the fragment and
keeps the reference, instead of deleting the whole thing.

More fixers land in real-world impact order. See
[epubveri](https://github.com/veripublica/epubveri) for the full catalogue of
what can be detected.

---

## Safety guarantees

These invariants hold for every fixer:

- **No mutation without an approved fix.** In the default mode you approve each
  one; `--auto-safe` auto-approves only *AutoSafe* fixes; `--yes` approves all.
- **Surgical and content-preserving.** A fix edits only what it must. An entry
  no fix touched is never decompressed and recompressed — its compressed bytes,
  compression method and timestamp are copied through as-is, and entry order and
  directory entries are kept. (The zip local headers themselves are rebuilt by
  the writer, so a repaired container is not *byte*-identical to its input; every
  byte of every entry's data is.)
- **The container is never quietly normalized.** epubsana does not repackage
  anything as a side effect of writing output — not even a `mimetype` entry that
  violates OCF. If your packaging is wrong, epubveri reports it and it stays
  reported; a defect epubsana did not propose and you did not approve is a defect
  epubsana did not touch.
- **Never guess.** If a finding has no safe, determinate fix, epubsana declines
  it rather than risk the content.
- **Independently re-validated.** After applying fixes, the whole book is
  re-checked with epubveri for the before → after counts — the tool proves its
  own result rather than asserting it.
- **The original isn't modified in place.** Repairs are written to a separate
  output file (by default `<input-stem>_fixed.epub`; overridden only if you
  point `-o` at another path), and an existing file there is never silently
  replaced — epubsana refuses until you pass `-f`.

---

## Reference standard

epubsana repairs toward **EPUB 3.3** (the current W3C Recommendation) and the
latest epubcheck rules. When both EPUB 2 and EPUB 3 forms would be valid, it
emits the most-current one — e.g. an encoding declaration becomes the HTML5
`<meta charset="utf-8">`, not the legacy `<meta http-equiv="Content-Type">`.

It does **not** rewrite legacy *features* wholesale (it won't drop the NCX for a
navigation document, for instance) — that is modernization, and belongs to
epublift. epubsana makes the legacy artifact *valid*, in place.

---

## Using epubsana as a Rust library

Every frontend shares one core crate, so a library consumer (epublift, your own
tool) gets identical behavior. The `Confirmer` trait is how "confirm each step"
lives in the core: you decide, per fix, whether to apply it.

```rust
use epubsana::{repair, Confirmer, Decision, Goal, Policy, ProposedFix, Workspace};

// A confirmer that approves everything (like the CLI's `--yes`). A real UI
// would inspect `fix.title`, `fix.rationale`, and `fix.preview` and ask a human.
struct ApproveAll;
impl Confirmer for ApproveAll {
    fn decide(&mut self, _fix: &ProposedFix) -> Decision {
        Decision::Approve
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read("book.epub")?;
    let mut ws = Workspace::load(&bytes)?;

    let mut confirmer = ApproveAll;
    let report = repair(&mut ws, Goal::Valid, Policy::AskEach, &mut confirmer)?;

    println!(
        "fatal {} → {}, error {} → {}",
        report.fatals_before, report.fatals_after,
        report.errors_before, report.errors_after,
    );
    for fix in &report.fixes {
        println!("{}: {}", fix.outcome.as_str(), fix.title);
    }

    if report.changed() {
        std::fs::write("book_fixed.epub", ws.serialize()?)?;
    }
    Ok(())
}
```

Key types:

- **`Workspace`** — the mutable, fidelity-preserving EPUB. `load(bytes)` reads,
  `serialize()` writes it back, `detect()` runs epubveri over the current state.
- **`repair(ws, goal, policy, confirmer)`** — the whole contract in one call:
  detect → propose → confirm (per `policy`) → apply → return a `ChangeReport`.
- **`Policy`** — `AskEach` (ask the confirmer for every fix),
  `AutoSafeThenAsk` (auto-apply *AutoSafe*, ask for the rest), or `DryRun`
  (propose and report, apply nothing).
- **`Confirmer`** — implement `decide(&mut self, fix: &ProposedFix) -> Decision`
  (`Approve` / `Reject`).
- **`ChangeReport`** — `fixes` (each a `ReportedFix` carrying its `Outcome`:
  `Applied` / `Skipped` / `Proposed`), `fatals_before`/`fatals_after`,
  `errors_before`/`errors_after`, `goal`, and `goal_met` — the tool's `0`/`1`
  line. `applied()` / `skipped()` / `changed()` are conveniences over `fixes`.
- **`Goal::is_met(&report)`** — `Valid` = no fatals and no errors; `Openable` =
  no fatals (the book opens).
- **`envelope`** — the shared machine shape. The skeleton is epubveri's
  reference type (`epubveri::envelope`, generic over the two tool-owned slots);
  epubsana supplies its own `Summary` and `Data` and maps a `ChangeReport` into
  it, so a library consumer emits exactly the JSON the CLI does — and the family
  keeps one copy of the envelope, not one per tool.
- **`fixers::plan(&report, &ws, goal)`** — build the proposals directly (what
  `--dry-run` uses) without applying anything.

---

## Known limitations

- **Coverage is partial and honest.** epubsana fixes the safely-fixable
  findings and reports the rest; a repaired book is not guaranteed fully valid.
  How much a given library improves varies — the tool clears what it can
  *safely* and leaves the rest reported, untouched.
- **Fixes are planned once, up front.** All proposals are built from the initial
  detection. A structural fixer that can't parse a document *before* an earlier
  fixer would have cleaned it up may decline it. (Re-planning after each fix is
  on the roadmap.)
- **`--goal` decides success, not yet fixer selection.** Both goals propose the
  same fixes today; what differs is the bar the result is measured against (see
  [Exit codes](#exit-codes)).
