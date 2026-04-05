//! Volume construction from decoded DICOM frames.

use crate::dicom_decode::DecodedFrame;
use crate::metadata::{FrameMetadata, VolumeGeometry};
use glam::{DMat3, DVec3, UVec3};
use std::cmp::Ordering;
use thiserror::Error;
use volren_core::{Volume, VolumeError};

/// Errors raised while assembling a 3D volume from decoded frames.
#[derive(Debug, Error)]
pub enum VolumeAssemblyError {
    /// No decoded frames were supplied.
    #[error("no decoded frames were provided")]
    EmptyFrames,
    /// A frame has incompatible geometry for the target volume.
    #[error("frame {frame_index} has inconsistent geometry: {reason}")]
    InconsistentGeometry {
        /// Index of the inconsistent frame.
        frame_index: usize,
        /// Human-readable mismatch description.
        reason: String,
    },
    /// One frame contains the wrong number of grayscale voxels.
    #[error("frame {frame_index} has {actual} voxels, expected {expected}")]
    PixelCountMismatch {
        /// Zero-based frame index.
        frame_index: usize,
        /// Expected pixel count.
        expected: usize,
        /// Actual decoded pixel count.
        actual: usize,
    },
    /// The final volume buffer was invalid.
    #[error(transparent)]
    Volume(#[from] VolumeError),
}

/// Assembles a grayscale `Volume<i16>` from decoded DICOM frames.
pub fn assemble_volume_from_frames(
    frames: &[DecodedFrame],
) -> Result<Volume<i16>, VolumeAssemblyError> {
    let geometry = derive_volume_geometry_from_frames(frames)?;
    let sorted_frames = sort_frames_for_volume(frames);
    let slice_len = geometry.slice_len();
    let mut slices = Vec::with_capacity(sorted_frames.len());

    for (index, frame) in sorted_frames.iter().enumerate() {
        if frame.pixels.len() != slice_len {
            return Err(VolumeAssemblyError::PixelCountMismatch {
                frame_index: index,
                expected: slice_len,
                actual: frame.pixels.len(),
            });
        }
        slices.push(frame.pixels.as_slice());
    }

    Volume::from_slices(
        &slices,
        geometry.dimensions.x,
        geometry.dimensions.y,
        geometry.spacing,
        geometry.origin,
        geometry.direction,
    )
    .map_err(VolumeAssemblyError::from)
}

/// Derives volume geometry from a homogeneous set of decoded frames.
pub fn derive_volume_geometry_from_frames(
    frames: &[DecodedFrame],
) -> Result<VolumeGeometry, VolumeAssemblyError> {
    let first = frames.first().ok_or(VolumeAssemblyError::EmptyFrames)?;
    validate_frame_set(frames, &first.metadata)?;
    let sorted_frames = sort_frames_for_volume(frames);
    let spacing = extract_spacing(&sorted_frames);
    let origin = sorted_frames
        .first()
        .and_then(|frame| frame.metadata.image_position)
        .unwrap_or(DVec3::ZERO);
    let direction = sorted_frames
        .first()
        .map(|frame| frame.metadata.direction())
        .unwrap_or(DMat3::IDENTITY);

    Ok(VolumeGeometry {
        dimensions: UVec3::new(
            first.metadata.columns as u32,
            first.metadata.rows as u32,
            sorted_frames.len() as u32,
        ),
        spacing,
        origin,
        direction,
    })
}

fn validate_frame_set(
    frames: &[DecodedFrame],
    first: &FrameMetadata,
) -> Result<(), VolumeAssemblyError> {
    for (index, frame) in frames.iter().enumerate() {
        if frame.metadata.rows != first.rows || frame.metadata.columns != first.columns {
            return Err(VolumeAssemblyError::InconsistentGeometry {
                frame_index: index,
                reason: format!(
                    "expected {}x{}, got {}x{}",
                    first.columns, first.rows, frame.metadata.columns, frame.metadata.rows
                ),
            });
        }
        if frame.metadata.samples_per_pixel != first.samples_per_pixel {
            return Err(VolumeAssemblyError::InconsistentGeometry {
                frame_index: index,
                reason: format!(
                    "expected samples_per_pixel={}, got {}",
                    first.samples_per_pixel, frame.metadata.samples_per_pixel
                ),
            });
        }
        if frame.metadata.image_orientation != first.image_orientation
            && frame.metadata.image_orientation.is_some()
            && first.image_orientation.is_some()
        {
            return Err(VolumeAssemblyError::InconsistentGeometry {
                frame_index: index,
                reason: "image orientation differs across frames".to_string(),
            });
        }
    }
    Ok(())
}

fn sort_frames_for_volume<'a>(frames: &'a [DecodedFrame]) -> Vec<&'a DecodedFrame> {
    let mut sorted: Vec<&DecodedFrame> = frames.iter().collect();
    if sorted.len() <= 1 {
        return sorted;
    }

    let Some(reference_normal) = reference_slice_normal(&sorted) else {
        sorted.sort_by(|left, right| compare_frames_fallback(left, right));
        return sorted;
    };

    let reference_origin = sorted
        .iter()
        .find_map(|frame| frame.metadata.image_position)
        .unwrap_or(DVec3::ZERO);

    sorted.sort_by(|left, right| {
        compare_frames_by_geometry(left, right, reference_origin, reference_normal)
            .then_with(|| compare_frames_fallback(left, right))
    });
    sorted
}

fn reference_slice_normal(frames: &[&DecodedFrame]) -> Option<DVec3> {
    let candidate = frames
        .iter()
        .find_map(|frame| frame.metadata.slice_normal())?;
    frames
        .iter()
        .all(|frame| {
            let Some(normal) = frame.metadata.slice_normal() else {
                return false;
            };
            normal.dot(candidate).abs() > 0.999
        })
        .then_some(candidate)
}

fn compare_frames_by_geometry(
    left: &DecodedFrame,
    right: &DecodedFrame,
    reference_origin: DVec3,
    reference_normal: DVec3,
) -> Ordering {
    match (left.metadata.image_position, right.metadata.image_position) {
        (Some(left_pos), Some(right_pos)) => {
            let left_distance = reference_normal.dot(left_pos - reference_origin);
            let right_distance = reference_normal.dot(right_pos - reference_origin);
            left_distance
                .partial_cmp(&right_distance)
                .unwrap_or(Ordering::Equal)
        }
        _ => compare_frames_fallback(left, right),
    }
}

fn compare_frames_fallback(left: &DecodedFrame, right: &DecodedFrame) -> Ordering {
    left.metadata
        .instance_number
        .cmp(&right.metadata.instance_number)
        .then_with(|| {
            left.metadata
                .sop_instance_uid
                .as_deref()
                .unwrap_or("")
                .cmp(right.metadata.sop_instance_uid.as_deref().unwrap_or(""))
        })
        .then_with(|| left.metadata.frame_index.cmp(&right.metadata.frame_index))
}

fn extract_spacing(frames: &[&DecodedFrame]) -> DVec3 {
    let first = &frames[0].metadata;
    let pixel_spacing = first.pixel_spacing.unwrap_or((1.0, 1.0));
    let slice_spacing = if frames.len() >= 2 {
        projected_slice_spacing(frames)
            .or(first.slice_thickness)
            .unwrap_or(1.0)
    } else {
        first.slice_thickness.unwrap_or(1.0)
    };

    DVec3::new(pixel_spacing.1, pixel_spacing.0, slice_spacing)
}

fn projected_slice_spacing(frames: &[&DecodedFrame]) -> Option<f64> {
    let normal = reference_slice_normal(frames)?;
    let first = frames.first()?.metadata.image_position?;
    let second = frames.get(1)?.metadata.image_position?;
    Some(normal.dot(second - first).abs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::FrameMetadata;
    use dicom_toolkit_image::PixelRepresentation;
    use volren_core::VolumeInfo;

    fn frame(
        instance_number: i32,
        z: f64,
        pixels: [i16; 4],
        orientation: Option<(DVec3, DVec3)>,
    ) -> DecodedFrame {
        DecodedFrame {
            metadata: FrameMetadata {
                frame_index: 0,
                rows: 2,
                columns: 2,
                number_of_frames: 1,
                samples_per_pixel: 1,
                bits_allocated: 16,
                bits_stored: 16,
                high_bit: 15,
                pixel_representation: PixelRepresentation::Signed,
                instance_number,
                pixel_spacing: Some((0.6, 0.8)),
                slice_thickness: Some(1.5),
                image_position: Some(DVec3::new(0.0, 0.0, z)),
                image_orientation: orientation,
                window_center: None,
                window_width: None,
                rescale_intercept: 0.0,
                rescale_slope: 1.0,
                sop_instance_uid: Some(format!("1.2.3.{instance_number}")),
                transfer_syntax_uid: "1.2.840.10008.1.2.1".to_string(),
            },
            pixels: pixels.to_vec(),
        }
    }

    #[test]
    fn assembles_sorted_volume() {
        let orientation = Some((DVec3::X, DVec3::Y));
        let frames = vec![
            frame(2, 2.0, [5, 6, 7, 8], orientation),
            frame(1, 1.0, [1, 2, 3, 4], orientation),
            frame(3, 3.0, [9, 10, 11, 12], orientation),
        ];

        let volume = assemble_volume_from_frames(&frames).expect("volume");
        assert_eq!(volume.dimensions(), UVec3::new(2, 2, 3));
        assert_eq!(volume.spacing(), DVec3::new(0.8, 0.6, 1.0));
        assert_eq!(volume.get(0, 0, 0), Some(1));
        assert_eq!(volume.get(1, 1, 2), Some(12));
    }

    #[test]
    fn falls_back_to_instance_number_sorting() {
        let mut frames = vec![
            frame(3, 0.0, [9, 10, 11, 12], None),
            frame(1, 0.0, [1, 2, 3, 4], None),
            frame(2, 0.0, [5, 6, 7, 8], None),
        ];
        for decoded in &mut frames {
            decoded.metadata.image_position = None;
        }

        let volume = assemble_volume_from_frames(&frames).expect("volume");
        assert_eq!(volume.get(0, 0, 0), Some(1));
        assert_eq!(volume.get(0, 0, 1), Some(5));
        assert_eq!(volume.get(0, 0, 2), Some(9));
    }

    #[test]
    fn rejects_inconsistent_geometry() {
        let orientation = Some((DVec3::X, DVec3::Y));
        let mut frames = vec![
            frame(1, 1.0, [1, 2, 3, 4], orientation),
            frame(2, 2.0, [5, 6, 7, 8], orientation),
        ];
        frames[1].metadata.columns = 3;

        let err = assemble_volume_from_frames(&frames).unwrap_err();
        assert!(matches!(
            err,
            VolumeAssemblyError::InconsistentGeometry { .. }
        ));
    }
}
