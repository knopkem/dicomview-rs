#![cfg(target_arch = "wasm32")]

use dicom_toolkit_data::{DataSet, Element, FileFormat, PixelData, Value};
use dicom_toolkit_dict::{tags, Vr};
use dicomview_wasm::decode_dicom_pixels;
use wasm_bindgen_test::wasm_bindgen_test;

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
    ds.set_string(dicomview_core::metadata::PIXEL_SPACING, Vr::DS, "0.5\\0.5");
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

#[wasm_bindgen_test]
fn decodes_pixels_through_wasm_export() {
    let pixels = decode_dicom_pixels(&encode_dataset(single_frame_dataset())).expect("decode");
    assert_eq!(pixels, vec![-924, -824, -724, -624]);
}
