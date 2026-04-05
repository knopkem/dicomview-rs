//! DICOM geometry and per-frame metadata extraction.

use dicom_toolkit_data::{DataSet, Value};
use dicom_toolkit_dict::{tags, Tag};
use dicom_toolkit_image::PixelRepresentation;
use glam::{DMat3, DVec3, UVec3};
use thiserror::Error;

/// DICOM tag `(0028,0030)` Pixel Spacing.
pub const PIXEL_SPACING: Tag = Tag::new(0x0028, 0x0030);

/// DICOM tag `(0018,0050)` Slice Thickness.
pub const SLICE_THICKNESS: Tag = Tag::new(0x0018, 0x0050);

/// Errors raised while extracting metadata from a DICOM dataset.
#[derive(Debug, Error, PartialEq)]
pub enum MetadataError {
    /// A required dataset attribute was missing.
    #[error("missing mandatory attribute: {name}")]
    MissingAttribute {
        /// Human-readable attribute name.
        name: &'static str,
    },
}

/// Geometry needed to allocate or build a 3D volume.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumeGeometry {
    /// Volume dimensions in voxels.
    pub dimensions: UVec3,
    /// Voxel spacing in world units, typically millimetres.
    pub spacing: DVec3,
    /// World-space origin of voxel `(0, 0, 0)`.
    pub origin: DVec3,
    /// Orientation matrix whose columns are the volume axes.
    pub direction: DMat3,
}

impl VolumeGeometry {
    /// Returns the number of scalar voxels in the full volume.
    #[must_use]
    pub fn voxel_count(self) -> usize {
        (self.dimensions.x as usize) * (self.dimensions.y as usize) * (self.dimensions.z as usize)
    }

    /// Returns the number of voxels contained in one Z slice.
    #[must_use]
    pub fn slice_len(self) -> usize {
        (self.dimensions.x as usize) * (self.dimensions.y as usize)
    }
}

/// Metadata extracted for one decoded image frame.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameMetadata {
    /// Zero-based frame index inside the source DICOM object.
    pub frame_index: u32,
    /// Number of rows in the frame.
    pub rows: u16,
    /// Number of columns in the frame.
    pub columns: u16,
    /// Number of frames in the source DICOM object.
    pub number_of_frames: u32,
    /// Samples per pixel, usually `1` for grayscale volumes.
    pub samples_per_pixel: u16,
    /// Bits allocated per sample.
    pub bits_allocated: u16,
    /// Bits stored per sample.
    pub bits_stored: u16,
    /// Highest stored bit index.
    pub high_bit: u16,
    /// Pixel sign convention.
    pub pixel_representation: PixelRepresentation,
    /// Instance Number `(0020,0013)` if present.
    pub instance_number: i32,
    /// Pixel spacing as `(row_spacing, column_spacing)`.
    pub pixel_spacing: Option<(f64, f64)>,
    /// Slice thickness if present.
    pub slice_thickness: Option<f64>,
    /// Image Position (Patient) if present.
    pub image_position: Option<DVec3>,
    /// Image Orientation (Patient) as `(row_direction, column_direction)`.
    pub image_orientation: Option<(DVec3, DVec3)>,
    /// Optional display window center.
    pub window_center: Option<f64>,
    /// Optional display window width.
    pub window_width: Option<f64>,
    /// Modality LUT intercept.
    pub rescale_intercept: f64,
    /// Modality LUT slope.
    pub rescale_slope: f64,
    /// SOP Instance UID if present.
    pub sop_instance_uid: Option<String>,
    /// Transfer Syntax UID from the file meta information.
    pub transfer_syntax_uid: String,
}

impl FrameMetadata {
    /// Returns a best-effort 3x3 direction matrix for the frame.
    #[must_use]
    pub fn direction(&self) -> DMat3 {
        self.image_orientation
            .map(|(row, col)| {
                let normal = row.cross(col).normalize_or_zero();
                if normal.length_squared() > 0.0 {
                    DMat3::from_cols(row, col, normal)
                } else {
                    DMat3::IDENTITY
                }
            })
            .unwrap_or(DMat3::IDENTITY)
    }

    /// Returns the frame normal when orientation is available.
    #[must_use]
    pub fn slice_normal(&self) -> Option<DVec3> {
        self.image_orientation.and_then(|(row, col)| {
            let normal = row.cross(col).normalize_or_zero();
            (normal.length_squared() > 0.0).then_some(normal)
        })
    }

    /// Returns in-plane spacing in `(x, y)` volume order.
    #[must_use]
    pub fn spacing_xy(&self) -> DVec3 {
        let (row, col) = self.pixel_spacing.unwrap_or((1.0, 1.0));
        DVec3::new(col, row, self.slice_thickness.unwrap_or(1.0))
    }

    /// Returns the expected pixel count for one decoded frame.
    #[must_use]
    pub fn voxel_count(&self) -> usize {
        (self.rows as usize) * (self.columns as usize) * (self.samples_per_pixel as usize)
    }
}

/// Extracts per-frame metadata from a parsed DICOM dataset.
pub fn extract_frame_metadata(
    dataset: &DataSet,
    transfer_syntax_uid: impl Into<String>,
    frame_index: u32,
) -> Result<FrameMetadata, MetadataError> {
    let rows = dataset
        .get_u16(tags::ROWS)
        .ok_or(MetadataError::MissingAttribute {
            name: "Rows (0028,0010)",
        })?;
    let columns = dataset
        .get_u16(tags::COLUMNS)
        .ok_or(MetadataError::MissingAttribute {
            name: "Columns (0028,0011)",
        })?;
    let bits_allocated =
        dataset
            .get_u16(tags::BITS_ALLOCATED)
            .ok_or(MetadataError::MissingAttribute {
                name: "BitsAllocated (0028,0100)",
            })?;
    let samples_per_pixel = dataset.get_u16(tags::SAMPLES_PER_PIXEL).unwrap_or(1);
    let bits_stored = dataset.get_u16(tags::BITS_STORED).unwrap_or(bits_allocated);
    let high_bit = dataset
        .get_u16(tags::HIGH_BIT)
        .unwrap_or(bits_stored.saturating_sub(1));
    let pixel_representation = match dataset.get_u16(tags::PIXEL_REPRESENTATION).unwrap_or(0) {
        1 => PixelRepresentation::Signed,
        _ => PixelRepresentation::Unsigned,
    };

    Ok(FrameMetadata {
        frame_index,
        rows,
        columns,
        number_of_frames: number_of_frames(dataset),
        samples_per_pixel,
        bits_allocated,
        bits_stored,
        high_bit,
        pixel_representation,
        instance_number: dataset.get_i32(tags::INSTANCE_NUMBER).unwrap_or(0),
        pixel_spacing: decimal_pair(dataset, PIXEL_SPACING),
        slice_thickness: dataset
            .get_f64(SLICE_THICKNESS)
            .or_else(|| dataset.get_string(SLICE_THICKNESS).and_then(parse_decimal)),
        image_position: decimal_vector3(dataset, tags::IMAGE_POSITION_PATIENT),
        image_orientation: orientation_pair(dataset, tags::IMAGE_ORIENTATION_PATIENT),
        window_center: decimal_value(dataset, tags::WINDOW_CENTER),
        window_width: decimal_value(dataset, tags::WINDOW_WIDTH),
        rescale_intercept: decimal_value(dataset, tags::RESCALE_INTERCEPT).unwrap_or(0.0),
        rescale_slope: decimal_value(dataset, tags::RESCALE_SLOPE).unwrap_or(1.0),
        sop_instance_uid: dataset
            .get_string(tags::SOP_INSTANCE_UID)
            .map(std::string::ToString::to_string),
        transfer_syntax_uid: transfer_syntax_uid.into(),
    })
}

fn number_of_frames(dataset: &DataSet) -> u32 {
    dataset
        .get(tags::NUMBER_OF_FRAMES)
        .and_then(|elem| match &elem.value {
            dicom_toolkit_data::Value::Ints(values) => {
                values.first().copied().map(|n| n.max(1) as u32)
            }
            dicom_toolkit_data::Value::Strings(values) => values
                .first()
                .and_then(|value| value.trim().parse::<u32>().ok()),
            dicom_toolkit_data::Value::U16(values) => values.first().copied().map(u32::from),
            dicom_toolkit_data::Value::U32(values) => values.first().copied(),
            _ => None,
        })
        .unwrap_or(1)
}

fn decimal_value(dataset: &DataSet, tag: Tag) -> Option<f64> {
    decimal_values(dataset, tag).into_iter().next()
}

fn parse_decimal(value: &str) -> Option<f64> {
    value.trim().parse::<f64>().ok()
}

fn decimal_values(dataset: &DataSet, tag: Tag) -> Vec<f64> {
    let Some(element) = dataset.get(tag) else {
        return Vec::new();
    };
    match &element.value {
        Value::Decimals(values) => values.clone(),
        Value::F64(values) => values.clone(),
        Value::F32(values) => values.iter().map(|&value| value as f64).collect(),
        Value::Strings(values) => values
            .iter()
            .flat_map(|value| value.split('\\'))
            .filter_map(parse_decimal)
            .collect(),
        Value::U16(values) => values.iter().map(|&value| value as f64).collect(),
        Value::U32(values) => values.iter().map(|&value| value as f64).collect(),
        Value::I32(values) => values.iter().map(|&value| value as f64).collect(),
        _ => Vec::new(),
    }
}

fn decimal_pair(dataset: &DataSet, tag: Tag) -> Option<(f64, f64)> {
    let parts = decimal_values(dataset, tag);
    (parts.len() >= 2).then(|| (parts[0], parts[1]))
}

fn decimal_vector3(dataset: &DataSet, tag: Tag) -> Option<DVec3> {
    let parts = decimal_values(dataset, tag);
    (parts.len() >= 3).then(|| DVec3::new(parts[0], parts[1], parts[2]))
}

fn orientation_pair(dataset: &DataSet, tag: Tag) -> Option<(DVec3, DVec3)> {
    let parts = decimal_values(dataset, tag);
    (parts.len() >= 6).then(|| {
        (
            DVec3::new(parts[0], parts[1], parts[2]).normalize_or_zero(),
            DVec3::new(parts[3], parts[4], parts[5]).normalize_or_zero(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dicom_toolkit_data::DataSet;
    use dicom_toolkit_dict::Vr;

    fn dataset_with_geometry() -> DataSet {
        let mut ds = DataSet::new();
        ds.set_u16(tags::ROWS, 4);
        ds.set_u16(tags::COLUMNS, 3);
        ds.set_u16(tags::SAMPLES_PER_PIXEL, 1);
        ds.set_u16(tags::BITS_ALLOCATED, 16);
        ds.set_u16(tags::BITS_STORED, 12);
        ds.set_u16(tags::HIGH_BIT, 11);
        ds.set_u16(tags::PIXEL_REPRESENTATION, 1);
        ds.set_string(PIXEL_SPACING, Vr::DS, "0.7\\0.8");
        ds.set_string(tags::IMAGE_POSITION_PATIENT, Vr::DS, "1\\2\\3");
        ds.set_string(tags::IMAGE_ORIENTATION_PATIENT, Vr::DS, "1\\0\\0\\0\\1\\0");
        ds.set_string(tags::WINDOW_CENTER, Vr::DS, "40");
        ds.set_string(tags::WINDOW_WIDTH, Vr::DS, "400");
        ds.set_string(tags::SOP_INSTANCE_UID, Vr::UI, "1.2.3");
        ds
    }

    #[test]
    fn extracts_frame_metadata() {
        let metadata = extract_frame_metadata(&dataset_with_geometry(), "1.2.840.10008.1.2.1", 0)
            .expect("metadata");
        assert_eq!(metadata.rows, 4);
        assert_eq!(metadata.columns, 3);
        assert_eq!(metadata.bits_allocated, 16);
        assert_eq!(metadata.spacing_xy(), DVec3::new(0.8, 0.7, 1.0));
        assert_eq!(metadata.image_position, Some(DVec3::new(1.0, 2.0, 3.0)));
        assert_eq!(metadata.sop_instance_uid.as_deref(), Some("1.2.3"));
    }

    #[test]
    fn direction_defaults_to_identity_without_orientation() {
        let mut metadata =
            extract_frame_metadata(&dataset_with_geometry(), "1.2.840.10008.1.2.1", 0).unwrap();
        metadata.image_orientation = None;
        assert_eq!(metadata.direction(), DMat3::IDENTITY);
        assert_eq!(metadata.slice_normal(), None);
    }
}
