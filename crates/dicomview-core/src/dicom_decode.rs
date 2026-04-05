//! DICOM parsing, decompression, and modality-space voxel decoding.

use crate::metadata::{extract_frame_metadata, FrameMetadata, MetadataError};
use dicom_toolkit_codec::decode_pixel_data;
use dicom_toolkit_core::error::DcmError;
use dicom_toolkit_data::{io::DicomReader, DataSet, FileFormat, PixelData, Value};
use dicom_toolkit_dict::tags;
use dicom_toolkit_image::{pixel, ModalityLut, PixelRepresentation};
use std::io::Cursor;
use thiserror::Error;

/// One decoded grayscale image frame plus its extracted metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedFrame {
    /// Geometry and decoding metadata for this frame.
    pub metadata: FrameMetadata,
    /// Pixel values after decompression and modality LUT application.
    pub pixels: Vec<i16>,
}

/// Errors raised while decoding DICOM bytes into voxels.
#[derive(Debug, Error)]
pub enum DicomDecodeError {
    /// The DICOM payload could not be parsed or decompressed.
    #[error(transparent)]
    Dicom(#[from] DcmError),
    /// Frame metadata could not be extracted.
    #[error(transparent)]
    Metadata(#[from] MetadataError),
    /// The image uses an unsupported sample layout for volumetric decoding.
    #[error(
        "unsupported samples per pixel {samples_per_pixel}; only grayscale images are supported"
    )]
    UnsupportedSamplesPerPixel {
        /// Unsupported sample count.
        samples_per_pixel: u16,
    },
    /// The image uses a pixel storage layout that is not yet supported.
    #[error("unsupported BitsAllocated={bits_allocated} for volumetric decoding")]
    UnsupportedBitsAllocated {
        /// Unsupported bit depth.
        bits_allocated: u16,
    },
    /// The frame count does not match the available native bytes.
    #[error(
        "pixel data length {actual} does not contain {frames} frame(s) of {bytes_per_frame} bytes"
    )]
    NativePixelLengthMismatch {
        /// Raw pixel byte length found in the dataset.
        actual: usize,
        /// Number of frames expected from metadata.
        frames: u32,
        /// Bytes required per frame.
        bytes_per_frame: usize,
    },
    /// No pixel data element was present in the dataset.
    #[error("missing pixel data element")]
    MissingPixelData,
    /// The decoded pixel count does not match the frame dimensions.
    #[error("decoded frame {frame_index} has {actual} pixels, expected {expected}")]
    PixelCountMismatch {
        /// Zero-based frame index.
        frame_index: u32,
        /// Expected grayscale voxel count.
        expected: usize,
        /// Actual number of decoded values.
        actual: usize,
    },
    /// A single-frame decode helper was used for a multi-frame payload.
    #[error("expected a single frame, found {actual_frames}")]
    ExpectedSingleFrame {
        /// Number of decoded frames in the source object.
        actual_frames: usize,
    },
}

/// Decodes every frame contained in a DICOM Part 10 payload.
pub fn decode_dicom(bytes: &[u8]) -> Result<Vec<DecodedFrame>, DicomDecodeError> {
    let file = DicomReader::new(Cursor::new(bytes)).read_file()?;
    decode_file(file)
}

/// Decodes a DICOM payload and requires that it contain exactly one frame.
pub fn decode_dicom_frame(bytes: &[u8]) -> Result<DecodedFrame, DicomDecodeError> {
    let mut frames = decode_dicom(bytes)?;
    if frames.len() == 1 {
        Ok(frames.remove(0))
    } else {
        Err(DicomDecodeError::ExpectedSingleFrame {
            actual_frames: frames.len(),
        })
    }
}

fn decode_file(file: FileFormat) -> Result<Vec<DecodedFrame>, DicomDecodeError> {
    let base_metadata =
        extract_frame_metadata(&file.dataset, file.meta.transfer_syntax_uid.clone(), 0)?;
    if base_metadata.samples_per_pixel != 1 {
        return Err(DicomDecodeError::UnsupportedSamplesPerPixel {
            samples_per_pixel: base_metadata.samples_per_pixel,
        });
    }

    let raw_frames = extract_raw_frames(
        &file.dataset,
        &base_metadata,
        &file.meta.transfer_syntax_uid,
    )?;
    let mut decoded_frames = Vec::with_capacity(raw_frames.len());
    for (frame_index, raw_frame) in raw_frames.iter().enumerate() {
        let mut metadata = extract_frame_metadata(
            &file.dataset,
            file.meta.transfer_syntax_uid.clone(),
            frame_index as u32,
        )?;
        metadata.frame_index = frame_index as u32;
        let pixels = decode_modality_voxels(
            &metadata,
            raw_frame,
            metadata.rows as usize * metadata.columns as usize,
        )?;
        decoded_frames.push(DecodedFrame { metadata, pixels });
    }
    Ok(decoded_frames)
}

fn extract_raw_frames(
    dataset: &DataSet,
    metadata: &FrameMetadata,
    transfer_syntax_uid: &str,
) -> Result<Vec<Vec<u8>>, DicomDecodeError> {
    let bytes_per_sample = (metadata.bits_allocated as usize).div_ceil(8);
    let bytes_per_frame = (metadata.rows as usize)
        * (metadata.columns as usize)
        * (metadata.samples_per_pixel as usize)
        * bytes_per_sample;
    let number_of_frames = metadata.number_of_frames.max(1);

    let pixel_data = dataset
        .find_element(tags::PIXEL_DATA)
        .map_err(|_| DicomDecodeError::MissingPixelData)?;

    match &pixel_data.value {
        Value::PixelData(PixelData::Native { bytes }) | Value::U8(bytes) => {
            if bytes.len() != bytes_per_frame * number_of_frames as usize {
                return Err(DicomDecodeError::NativePixelLengthMismatch {
                    actual: bytes.len(),
                    frames: number_of_frames,
                    bytes_per_frame,
                });
            }
            Ok(bytes
                .chunks_exact(bytes_per_frame)
                .map(|chunk| chunk.to_vec())
                .collect())
        }
        Value::PixelData(pixel_data @ PixelData::Encapsulated { .. }) => pixel_data
            .encapsulated_frames(number_of_frames)?
            .into_iter()
            .map(|compressed| {
                decode_pixel_data(
                    transfer_syntax_uid,
                    &compressed,
                    metadata.rows,
                    metadata.columns,
                    metadata.bits_allocated,
                    metadata.samples_per_pixel,
                )
                .map_err(DicomDecodeError::from)
            })
            .collect(),
        _ => Err(DicomDecodeError::MissingPixelData),
    }
}

fn decode_modality_voxels(
    metadata: &FrameMetadata,
    pixel_data: &[u8],
    expected_len: usize,
) -> Result<Vec<i16>, DicomDecodeError> {
    let modality_lut = ModalityLut::new(metadata.rescale_intercept, metadata.rescale_slope);

    let values = match (metadata.bits_allocated, metadata.pixel_representation) {
        (8, _) => modality_lut.apply_to_frame_u8(pixel_data),
        (16, PixelRepresentation::Unsigned) => {
            let pixels = pixel::decode_u16_le(pixel_data);
            let pixels = pixel::mask_u16(&pixels, metadata.bits_stored, metadata.high_bit);
            modality_lut.apply_to_frame_u16(&pixels)
        }
        (16, PixelRepresentation::Signed) => {
            let pixels = pixel::decode_i16_le(pixel_data);
            let pixels = pixel::mask_i16(&pixels, metadata.bits_stored, metadata.high_bit);
            modality_lut.apply_to_frame_i16(&pixels)
        }
        (bits_allocated, _) => {
            return Err(DicomDecodeError::UnsupportedBitsAllocated { bits_allocated });
        }
    };

    if values.len() < expected_len {
        return Err(DicomDecodeError::PixelCountMismatch {
            frame_index: metadata.frame_index,
            expected: expected_len,
            actual: values.len(),
        });
    }

    Ok(values
        .into_iter()
        .take(expected_len)
        .map(|value| value.round().clamp(i16::MIN as f64, i16::MAX as f64) as i16)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dicom_toolkit_data::{Element, PixelData, Value};
    use dicom_toolkit_dict::Vr;

    fn encode_dataset(ds: DataSet) -> Vec<u8> {
        let ff = FileFormat::from_dataset("1.2.840.10008.5.1.4.1.1.2", "1.2.3", ds);
        let mut buf = Vec::new();
        dicom_toolkit_data::DicomWriter::new(&mut buf)
            .write_file(&ff)
            .expect("encode");
        buf
    }

    fn single_frame_dataset() -> DataSet {
        let mut ds = DataSet::new();
        ds.set_u16(tags::ROWS, 2);
        ds.set_u16(tags::COLUMNS, 2);
        ds.set_u16(tags::SAMPLES_PER_PIXEL, 1);
        ds.set_u16(tags::BITS_ALLOCATED, 16);
        ds.set_u16(tags::BITS_STORED, 16);
        ds.set_u16(tags::HIGH_BIT, 15);
        ds.set_u16(tags::PIXEL_REPRESENTATION, 1);
        ds.set_string(tags::PHOTOMETRIC_INTERPRETATION, Vr::CS, "MONOCHROME2");
        ds.set_string(tags::IMAGE_POSITION_PATIENT, Vr::DS, "0\\0\\5");
        ds.set_string(tags::IMAGE_ORIENTATION_PATIENT, Vr::DS, "1\\0\\0\\0\\1\\0");
        ds.set_string(crate::metadata::PIXEL_SPACING, Vr::DS, "0.5\\0.5");
        ds.set_f64(tags::RESCALE_INTERCEPT, -1024.0);
        ds.set_f64(tags::RESCALE_SLOPE, 1.0);
        ds.insert(Element::new(
            tags::PIXEL_DATA,
            Vr::OW,
            Value::PixelData(PixelData::Native {
                bytes: bytemuck::cast_slice(&[100i16, 200, 300, 400]).to_vec(),
            }),
        ));
        ds
    }

    #[test]
    fn decodes_single_frame_native_pixels() {
        let frames = decode_dicom(&encode_dataset(single_frame_dataset())).expect("decode");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].pixels, vec![-924, -824, -724, -624]);
        assert_eq!(
            frames[0].metadata.image_position,
            Some(glam::DVec3::new(0.0, 0.0, 5.0))
        );
    }

    #[test]
    fn rejects_rgb_images_for_volume_decode() {
        let mut ds = single_frame_dataset();
        ds.set_u16(tags::SAMPLES_PER_PIXEL, 3);
        ds.insert(Element::new(
            tags::PIXEL_DATA,
            Vr::OB,
            Value::PixelData(PixelData::Native {
                bytes: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
            }),
        ));

        let err = decode_dicom(&encode_dataset(ds)).unwrap_err();
        assert!(matches!(
            err,
            DicomDecodeError::UnsupportedSamplesPerPixel {
                samples_per_pixel: 3
            }
        ));
    }
}
