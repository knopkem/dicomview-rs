# dicomview-rs

`dicomview-rs` is a standalone Rust/WASM medical-imaging engine for the web. It provides DICOM parsing and decompression, progressive volume assembly, MPR state, thick-slab rendering, and WebGPU volume rendering through a browser-friendly wasm facade and an npm package.

## Workspace layout

| Path | Purpose |
| --- | --- |
| `crates/dicomview-core` | WASM-safe DICOM decode, metadata extraction, incremental volumes, MPR/volume state, presets |
| `crates/dicomview-gpu` | Multi-viewport orchestration on top of `volren-rs`, including progressive 3D texture uploads |
| `crates/dicomview-wasm` | `wasm-bindgen` facade for browsers plus a simple WADO-RS loader |
| `js/` | Publishable `@knopkem/dicomview` package with viewer wrapper, DICOMweb loader, and decode workers |

## Implemented scope

- grayscale DICOM Part 10 decode through `dicom-toolkit-rs`
- progressive slice insertion into CPU and GPU volume storage
- axial / coronal / sagittal MPR state with shared crosshair support
- thick-slab MIP / MinIP / average slice rendering
- WebGPU volume rendering with built-in CT and MR presets
- single-canvas `StackViewer` for 2D stack browsing
- four-canvas `Viewer` for MPR + volume rendering
- aspect-ratio-correct slice rendering (no stretching)
- TypeScript `Viewer`, `StackViewer`, and `DICOMwebLoader` wrapper classes
- optional web-worker decode path in the npm loader

## Current limitations

- the built-in DICOMweb loaders assume **single-frame** instances
- volumetric decode currently supports **grayscale** source images only
- annotations, segmentation, and application-level tools are still out of scope
- WebGPU-capable browsers are required for rendering

## Build

```bash
cargo test --workspace
cargo check --workspace --target wasm32-unknown-unknown
cargo run --release --example benchmark_core -p dicomview-core

cd js
npm install
npm run build
```

## Publish

A single script handles the full release pipeline (Rust tests → WASM build → TS build → crates.io → npm):

```bash
# Full publish
./publish.sh

# Dry-run (builds everything, publishes nothing)
./publish.sh --dry-run

# npm only (skip crates.io — e.g. when only JS/TS changed)
./publish.sh --skip-crates

# crates.io only (skip npm)
./publish.sh --skip-npm
```

The script verifies that `Cargo.toml` and `package.json` versions match before proceeding.

## Package usage

```ts
import { DICOMwebLoader, Presets, Viewer, StackViewer } from "@knopkem/dicomview";

// --- Four-canvas MPR + volume rendering ---
const viewer = await Viewer.create({
  axial: document.getElementById("axial") as HTMLCanvasElement,
  coronal: document.getElementById("coronal") as HTMLCanvasElement,
  sagittal: document.getElementById("sagittal") as HTMLCanvasElement,
  volume: document.getElementById("volume") as HTMLCanvasElement,
});

// --- Single-canvas 2D stack browsing ---
const stack = await StackViewer.create({
  canvas: document.getElementById("canvas") as HTMLCanvasElement,
});

// --- Load a series ---
const loader = new DICOMwebLoader({
  wadoRoot: "https://pacs.example.com/dicom-web",
  decodeWorkers: 2,
});

await loader.loadSeries(viewer, {
  studyUid: "1.2.3",
  seriesUid: "4.5.6",
});

viewer.setVolumePreset(Presets.CT_SOFT_TISSUE);
viewer.setBlendMode("composite");
viewer.setThickSlab({
  viewport: "axial",
  projection: "mip",
  thickness: 8,
});
viewer.render();
```

## Additional docs

- `docs/architecture.md`
- `docs/browser-support.md`
- `examples/basic-mpr/`
- `examples/volume-rendering/`
