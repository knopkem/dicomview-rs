# @knopkem/dicomview

`@knopkem/dicomview` is the browser package for `dicomview-rs`. It wraps the Rust/WASM viewer, handles wasm initialization, and provides a DICOMweb loader with optional worker-based decode.

## Install

```bash
npm install @knopkem/dicomview
```

## Build (from source)

```bash
npm install
npm run build
```

`npm run build` performs two steps:

1. `wasm-pack build ../crates/dicomview-wasm --target web --out-dir ../../js/wasm`
2. `tsc -p tsconfig.json`

The wasm-pack output is emitted into `js/wasm/`, which is the directory shipped in the npm tarball.

## Publish

Use the top-level publish script from the repo root:

```bash
# Full publish (Rust tests → WASM → TS → crates.io → npm)
./publish.sh

# Dry-run (builds everything, publishes nothing)
./publish.sh --dry-run

# npm only (skip crates.io)
./publish.sh --skip-crates
```

Or publish manually:

```bash
npm run build
npm pack --dry-run    # inspect contents
npm publish --access public
```

## Public API

```ts
import {
  DICOMwebLoader,
  Presets,
  Viewer,
  StackViewer,
} from "@knopkem/dicomview";
```

- `Viewer.create(...)` — four-canvas MPR + volume renderer (axial, coronal, sagittal, volume)
- `StackViewer.create(...)` — single-canvas 2D stack viewer with scroll, window/level, and thick-slab
- `DICOMwebLoader` — streams a DICOMweb series into either viewer type
- `Presets` — built-in CT/MR transfer-function identifiers

## Notes

- the loader currently expects single-frame instances from DICOMweb
- worker decode is optional and enabled with `decodeWorkers > 0`
- rendering requires browser WebGPU support
