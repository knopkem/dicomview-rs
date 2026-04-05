//! Incremental 3D texture upload helpers.

use volren_gpu::{RenderError, VolumeRenderer};

/// Uploads one signed 16-bit slice into an already allocated volume texture.
pub fn update_texture_slice_i16(
    renderer: &mut VolumeRenderer,
    z_index: u32,
    pixels: &[i16],
    scalar_range: (f64, f64),
) -> Result<(), RenderError> {
    renderer.update_volume_slice_i16(z_index, pixels, scalar_range)
}
