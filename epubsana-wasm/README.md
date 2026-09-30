# epubsana-wasm

WebAssembly bindings for [**epubsana**](https://github.com/veripublica/epubsana) — a
pure-Rust EPUB repairer. Repair an `.epub` **entirely in the browser** (or any JS
runtime): no server round-trip, no native dependencies. **The bytes never
leave the page** — a real privacy guarantee for unpublished manuscripts.

It runs the same repair function as the command line: it proposes the same
fixes, you choose which to approve, and the run applies them, validates the
book, and undoes any fix that made a finding more frequent. The same book with
the same approvals comes back byte for byte what the CLI produces.

## Install

```
npm install @veripublica/epubsana-wasm
```

## Usage

`Session` mirrors epubsana's "confirm each step" contract:

```js
import { Session } from "@veripublica/epubsana-wasm";

const bytes = new Uint8Array(await file.arrayBuffer()); // a File / fetched .epub
const s = Session.load(bytes);

const { fatals_before, errors_before, fixes } = s.plan();
// fixes[i] = { index, tier: "AutoSafe" | "ConfirmNeeded", id, severity, title,
//              rationale, preview, outcome, reverted_for }

// The fixes the user approved, by index: here every safe one, plus one more.
const approved = fixes.filter((f) => f.tier === "AutoSafe").map((f) => f.index);
approved.push(fixes[2].index);

// Applies the approved fixes, validates the book, undoes any fix that made a
// finding more frequent, and returns the veripublica machine envelope's
// `inputs[i]` shape — the same object the CLI's `--format json` emits.
const r = s.repair(Uint32Array.from(approved), "valid"); // or "openable"
console.log(r.status, r.summary.errors_before, "→", r.summary.errors_after);
for (const item of r.items) console.log(item.outcome, item.severity, item.code);
// outcome: "applied" | "skipped" (not approved) | "reverted" (undone)

const repaired = s.result_bytes(); // Uint8Array — download as <name>_fixed.epub
```

`repair` always starts from the book as loaded, so calling it again with a
different selection replaces the previous run. `s.report(goal)` re-validates the
current bytes against another goal without repairing again.

**The error count can go up, and that is not damage.** A fix that lets the
validator see part of the book it could not see before (a package `version` no
validator recognises, or an entity that made a chapter unreadable) is not
undone for what then appears: those findings were always in the book.
`s.revealed()` returns how many. They were not in the plan, so repairing
`result_bytes()` again can fix some of them. The count is not in the envelope's
`summary`, which the conventions own; the CLI prints it as a note.

**Breaking change in 0.20.0:** `apply(index)` and `apply_auto_safe()` are gone.
They applied each fix the moment it was approved and never checked the result,
so a fix that made the book worse went into the download. Collect the approvals
and call `repair` once.

`severity` is **inherited** from the finding a fix addresses (epubveri's
five-value vocabulary: `fatal | error | warning | info | usage`) — it describes
the defect, not the fix. A **fatal** is what stops a book from opening at all, so
a fatal-only book has zero *errors* and is not remotely valid; count them apart.

Using it **directly in a browser without a bundler**? Build the `web` target
(`wasm-pack build --target web`), which exposes an async `init()` you `await`
once before constructing a `Session` — that's what `demo/index.html` uses.

## Build

```
wasm-pack build --target web      # for the demo / no-bundler use
wasm-pack build --target bundler  # for the npm package (webpack / Vite / Rollup)
```

The returned types ship with a real generated `.d.ts` (via `tsify`).

## License

Dual-licensed **AGPL-3.0-only OR a commercial license**, same as epubsana.
