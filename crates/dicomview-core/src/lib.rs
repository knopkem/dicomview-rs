//! WASM-safe core imaging primitives for `dicomview-rs`.

#![deny(missing_docs)]
#![deny(unsafe_code)]

pub mod dicom_decode;
pub mod incremental_volume;
pub mod metadata;
pub mod presets;
pub mod viewport_state;
pub mod volume_assembly;

pub use dicom_decode::{decode_dicom, decode_dicom_frame, DecodedFrame, DicomDecodeError};
pub use incremental_volume::{IncrementalVolume, IncrementalVolumeError};
pub use metadata::{extract_frame_metadata, FrameMetadata, MetadataError, VolumeGeometry};
pub use presets::{preset, preset_ids, VolumePreset, VolumePresetId};
pub use viewport_state::{
    SlicePreviewMode, SlicePreviewState, SliceProjectionMode, VolumeBlendMode, VolumeViewState,
};
pub use volume_assembly::{
    assemble_volume_from_frames, derive_volume_geometry_from_frames, VolumeAssemblyError,
};
