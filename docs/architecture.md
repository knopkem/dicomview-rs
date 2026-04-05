# dicomview-rs architecture

`dicomview-rs` is split into four layers:

1. `dicom-toolkit-rs` decodes DICOM bytes into modality-space pixels.
2. `dicomview-core` turns decoded frames into a progressive volume and stores pure MPR / volume interaction state.
3. `dicomview-gpu` owns the shared `volren-gpu` renderer, allocates the 3D texture once, and updates individual Z slices as they arrive.
4. `dicomview-wasm` and `@dicomview/core` expose the browser API.

## Data flow

1. DICOMweb metadata is fetched and converted into `VolumeGeometry`.
2. `Viewer.prepare_volume()` allocates CPU-side and GPU-side storage.
3. Each fetched DICOM instance is decoded either on the main thread or in a worker.
4. `feed_pixel_slice()` inserts the slice into `IncrementalVolume` and writes it into the existing GPU texture.
5. `render()` draws axial, coronal, sagittal, and volume viewports from one shared renderer.

## Design choices

- **Progressive upload**: the renderer does not recreate the whole texture for each slice.
- **Pure core math**: crosshair, scroll, thick-slab, zoom, pan, and orbit state stay outside the browser bindings.
- **Thin wasm facade**: the Rust wasm crate exposes low-level control; the npm wrapper provides the ergonomic browser API.
