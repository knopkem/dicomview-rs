use dicom_toolkit_data::{DataSet, Element, FileFormat, PixelData, Value};
use dicom_toolkit_dict::{tags, Vr};
use dicomview_core::{decode_dicom_frame, IncrementalVolume, VolumeGeometry};
use glam::{DMat3, DVec3, UVec3};
use std::time::Instant;

fn main() {
    let bytes = encode_dataset(test_dataset());
    let decode_iterations = 200;
    let insert_iterations = 100;

    let decode_start = Instant::now();
    let mut last_pixels = Vec::new();
    for _ in 0..decode_iterations {
        last_pixels = decode_dicom_frame(&bytes).expect("decode").pixels;
    }
    let decode_elapsed = decode_start.elapsed();

    let geometry = VolumeGeometry::new(
        UVec3::new(128, 128, insert_iterations),
        DVec3::ONE,
        DVec3::ZERO,
        DMat3::IDENTITY,
    );
    let insert_start = Instant::now();
    let mut volume = IncrementalVolume::new(geometry).expect("volume");
    for z in 0..insert_iterations {
        volume
            .insert_slice(z as u32, &last_pixels)
            .expect("insert slice");
    }
    let insert_elapsed = insert_start.elapsed();

    println!(
        "decode_dicom_frame: {} iterations in {:?} ({:.3} ms/iter)",
        decode_iterations,
        decode_elapsed,
        decode_elapsed.as_secs_f64() * 1_000.0 / decode_iterations as f64
    );
    println!(
        "IncrementalVolume::insert_slice: {} iterations in {:?} ({:.3} ms/iter)",
        insert_iterations,
        insert_elapsed,
        insert_elapsed.as_secs_f64() * 1_000.0 / insert_iterations as f64
    );
}

fn encode_dataset(ds: DataSet) -> Vec<u8> {
    let ff = FileFormat::from_dataset("1.2.840.10008.5.1.4.1.1.2", "1.2.3", ds);
    let mut buf = Vec::new();
    dicom_toolkit_data::DicomWriter::new(&mut buf)
        .write_file(&ff)
        .expect("encode");
    buf
}

fn test_dataset() -> DataSet {
    let mut ds = DataSet::new();
    ds.set_u16(tags::ROWS, 128);
    ds.set_u16(tags::COLUMNS, 128);
    ds.set_u16(tags::SAMPLES_PER_PIXEL, 1);
    ds.set_u16(tags::BITS_ALLOCATED, 16);
    ds.set_u16(tags::BITS_STORED, 16);
    ds.set_u16(tags::HIGH_BIT, 15);
    ds.set_u16(tags::PIXEL_REPRESENTATION, 1);
    ds.set_string(tags::PHOTOMETRIC_INTERPRETATION, Vr::CS, "MONOCHROME2");
    ds.set_string(tags::IMAGE_POSITION_PATIENT, Vr::DS, "0\\0\\0");
    ds.set_string(tags::IMAGE_ORIENTATION_PATIENT, Vr::DS, "1\\0\\0\\0\\1\\0");
    ds.set_string(dicomview_core::metadata::PIXEL_SPACING, Vr::DS, "1\\1");
    ds.set_f64(tags::RESCALE_INTERCEPT, -1024.0);
    ds.set_f64(tags::RESCALE_SLOPE, 1.0);

    let mut pixels = vec![0i16; 128 * 128];
    for y in 0..128usize {
        for x in 0..128usize {
            pixels[y * 128 + x] = ((x + y) % 2048) as i16;
        }
    }
    ds.insert(Element::new(
        tags::PIXEL_DATA,
        Vr::OW,
        Value::PixelData(PixelData::Native {
            bytes: bytemuck::cast_slice(&pixels).to_vec(),
        }),
    ));
    ds
}
