# epubsana-wasm

WebAssembly bindings for [**epubsana**](https://github.com/veripublica/epubsana) — a
pure-Rust EPUB repairer. Repair an `.epub` **entirely in the browser** (or any JS
runtime): no server round-trip, no native dependencies. **The bytes never
leave the page** — a real privacy guarantee for unpublished manuscripts.

It reuses the exact core epubsana uses on the command line, so the behaviour is
identical: it proposes safe fixes, you approve them, and it reports what changed.

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

const { errors_before, fixes } = s.state();
// fixes[i] = { index, tier: "AutoSafe" | "ConfirmNeeded", id, title, rationale, preview, applied }

s.apply_auto_safe();     // apply every provably-safe fix in one go
s.apply(fixes[2].index); // approve a specific ConfirmNeeded fix

console.log(errors_before, "→", s.errors_after());
const repaired = s.result_bytes(); // Uint8Array — download as <name>_fixed.epub
```

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
