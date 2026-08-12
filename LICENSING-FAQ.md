# Licensing FAQ

Short version: **if you are using these tools, or building something that calls
them, you are free. The commercial license exists for one narrow case — putting
our code inside a closed-source product you distribute, or running a modified
version as a network service.**

This document explains how we read `LICENSE` and how we intend it to apply. It
does not modify `LICENSE`; where the two ever disagree, `LICENSE` governs.

Applies to `epubsana` and `epubveri` — both AGPL-3.0-only OR a commercial
license.

One distinction worth knowing, because it makes half of this page unnecessary
for one of the two tools: **`epubveri` never writes to your book.** It reads and
reports. There is no path by which any part of it can end up inside an EPUB.
`epubsana` does write — that is its job — so the question of what ends up in your
file is a real one for it, and is answered below.

---

## Does the license cover the books I repair or check?

**No. Your files are yours, always, with no exceptions and no conditions.**

The AGPL — like the GPL — governs copying, modifying and distributing *the
software*. It says nothing about the data the software processes. Repairing an
EPUB with `epubsana` no more licenses that EPUB than GCC licenses the program it
compiles, or Sigil licenses the book you edit in it.

This holds regardless of what you do with the result. Sell it, ship it to a
retailer, give it away, keep it. You owe us nothing and you need no permission.

**And we checked the obvious follow-up before promising this unconditionally.**
`epubsana` repairs by editing your files, so it is fair to ask what of *ours*
ends up in them. The answer: no branding or generator string of any kind, no
generated identifiers or invented values, and no new files added to the
container — every edit is written back to a document that was already there. The
only text it authors is the structural markup a defect requires: a `<p>` wrapper
around stray text, a `<!DOCTYPE html>`, a `<meta charset="utf-8"/>`, an
identifier mechanically derived from one already in your book. Nothing
expressive, and nothing we would have any claim over even if the license reached
the output, which it does not.

## I sell the EPUBs I produce. Do I need a commercial license?

**No.** Not as a publisher, not as a retailer, not as a self-published author
selling your own book, not as a freelancer producing files for clients.

Using the tool commercially is not what the commercial license is for. See
[Who actually needs one](#who-actually-needs-the-commercial-license) below.

## Can a Sigil or calibre plugin use these tools? What license must the plugin be?

**Yes, and the plugin's license is entirely the plugin author's choice.**

A plugin that runs `epubsana` as a separate program — invoking the CLI and
reading its output, which is how we expect most integrations to work — is not
forming a combined work with it. Two programs talking at arm's length stay two
programs. Nothing propagates to the plugin, and nothing propagates to the editor
hosting it.

If instead you *link* our Rust crate directly into a GPLv3 program, that is also
explicitly permitted, and it is worth stating the clause exactly rather than
reassuringly:

- **AGPLv3 section 13** (our license) permits combining a covered work with a
  GPLv3 work and conveying the result; "the work with which it is combined will
  remain governed by version 3 of the GNU General Public License."
- **GPLv3 section 13** permits the same combination from the other side, and
  there "the special requirements of the GNU Affero General Public License,
  section 13, concerning interaction through a network will apply to **the
  combination as such**."

The two clauses are **not symmetrical**, and which one governs depends on which
side you approach from. We would rather you read that and check it than take a
softer summary from us.

In practice it rarely arises: see the next answer, and note that a network
requirement only bites where there is network interaction with a modified
version at all.

## Can *all* users of that plugin use it, including commercial ones?

**Yes. All of them, with no distinction between hobbyist and commercial use.**

We will never ask a user of Sigil, calibre, or any other editor for a license fee
because of what they produce with it. If that were the intent, publishing a
plugin-friendly CLI would be a trap, and we are not interested in setting one.

## Does the AGPL network clause affect me?

Almost certainly not. AGPL section 13 applies when you **modify** the program and
let users interact with **your modified version over a network**. Running an
unmodified `epubsana` on your own machine, inside an editor, or in your own
build pipeline is nowhere near that trigger.

## Who actually needs the commercial license

Two situations, both about *distributing or serving our code* rather than using
it:

1. **Embedding in a closed-source product you distribute** — an e-reader,
   a retailer's ingestion pipeline, a proprietary production tool — where you do
   not want the AGPL's source-availability obligations to reach your product.
2. **Offering a modified version as a network service** where you do not want to
   publish your modifications.

If you are not doing one of those, the AGPL is all you need, and it costs
nothing.

Commercial terms: **Baris Kayadelen** — baris@kayadelen.com. See
[`LICENSE-COMMERCIAL.md`](./LICENSE-COMMERCIAL.md).

## Why AGPL at all, if you are this permissive about use?

Because the two questions are different. The AGPL is not aimed at users; it is
aimed at the case where someone takes this work, closes it, and ships it as
their own with nothing flowing back. That has happened to the author before, and
the license is the protection against it happening again.

Nothing in that goal requires being difficult with the people building on top of
these tools, which is why this document exists.

---

*Written by the copyright holder as a statement of intent and of how we read our
own license. It is not legal advice, and it is not a substitute for reading
`LICENSE` yourself. If you need certainty for a specific commercial arrangement,
get it in writing from us — we would rather answer the question than have you
guess.*
