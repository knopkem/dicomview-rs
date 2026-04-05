# Browser support

`dicomview-rs` currently targets browsers with **WebGPU** enabled.

## Requirements

- WebGPU-capable Chromium, Edge, or Safari Technology Preview class browsers
- JavaScript modules
- Web Workers if `decodeWorkers` is enabled

## Current assumptions

- rendering uses WebGPU only; there is no WebGL fallback yet
- the bundled DICOMweb loader expects CORS-enabled DICOMweb endpoints
- worker decode expects the generated wasm bundle to be reachable from the package's `wasm/` directory

## Operational notes

- if a canvas changes size, the wasm viewer reconfigures its surface on the next render
- if worker creation is unavailable, the TypeScript loader falls back to main-thread decode automatically
