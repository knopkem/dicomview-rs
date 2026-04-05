# @dicomview/core

`@dicomview/core` is the browser package for `dicomview-rs`. It wraps the Rust/WASM viewer, handles wasm initialization, and provides a DICOMweb loader with optional worker-based decode.

## Build

```bash
npm install
npm run build
```

`npm run build` performs two steps:

1. `wasm-pack build ../crates/dicomview-wasm --target web --out-dir ./wasm`
2. `tsc -p tsconfig.json`

## Public API

```ts
import { DICOMwebLoader, Presets, Viewer } from "@dicomview/core";
```

- `Viewer.create(...)` mounts the four-canvas Rust/WebGPU renderer
- `DICOMwebLoader.loadSeries(...)` streams a DICOMweb series into the viewer
- `Presets` exposes the built-in CT/MR transfer-function identifiers

## Notes

- the loader currently expects single-frame instances from DICOMweb
- worker decode is optional and enabled with `decodeWorkers > 0`
- rendering requires browser WebGPU support
