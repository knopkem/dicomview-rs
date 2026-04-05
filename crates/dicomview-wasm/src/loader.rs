//! Optional browser-side DICOMweb loader.

use crate::{utils::js_error, viewer::Viewer};
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use glam::{DMat3, DVec3, UVec3};
#[cfg(target_arch = "wasm32")]
use js_sys::{encode_uri_component, Uint8Array};
#[cfg(target_arch = "wasm32")]
use js_sys::{Array, Reflect};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;

/// A simple WADO-RS series loader with progress reporting and abort support.
#[wasm_bindgen]
pub struct WadoLoader {
    loaded: usize,
    total: usize,
    #[cfg(target_arch = "wasm32")]
    abort_controller: Option<web_sys::AbortController>,
}

#[wasm_bindgen]
impl WadoLoader {
    /// Creates a new loader.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            loaded: 0,
            total: 0,
            #[cfg(target_arch = "wasm32")]
            abort_controller: None,
        }
    }

    /// Returns how many slices have been loaded so far.
    #[must_use]
    pub fn loaded(&self) -> usize {
        self.loaded
    }

    /// Returns the total number of slices expected.
    #[must_use]
    pub fn total(&self) -> usize {
        self.total
    }

    /// Aborts any active in-flight requests.
    pub fn abort(&mut self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(controller) = self.abort_controller.take() {
            controller.abort();
        }
    }

    /// Loads a single-frame DICOM series through WADO-RS metadata and instance retrieval.
    pub async fn load_series(
        &mut self,
        viewer: &mut Viewer,
        wado_root: &str,
        study_uid: &str,
        series_uid: &str,
    ) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (viewer, wado_root, study_uid, series_uid);
            Err(js_error(
                "WADO-RS loading is only supported on wasm32 targets",
            ))
        }

        #[cfg(target_arch = "wasm32")]
        {
            let controller = web_sys::AbortController::new()
                .map_err(|_| js_error("failed to create AbortController"))?;
            let signal = controller.signal();
            self.abort_controller = Some(controller);

            let metadata_url = format!(
                "{}/studies/{}/series/{}/metadata",
                trim_root(wado_root),
                encode_uri_component(study_uid),
                encode_uri_component(series_uid)
            );
            let metadata_json =
                fetch_json(&metadata_url, Some(&signal), "application/dicom+json").await?;
            let instances = parse_series_metadata(&metadata_json)?;
            if instances.is_empty() {
                self.abort_controller = None;
                return Err(js_error("metadata endpoint returned no instances"));
            }

            let geometry = derive_geometry(&instances)?;
            viewer.prepare_volume_native(geometry)?;
            self.loaded = 0;
            self.total = instances.len();

            for (slice_index, instance) in instances.iter().enumerate() {
                let instance_url = format!(
                    "{}/studies/{}/series/{}/instances/{}",
                    trim_root(wado_root),
                    encode_uri_component(study_uid),
                    encode_uri_component(series_uid),
                    encode_uri_component(&instance.sop_instance_uid)
                );
                let bytes = fetch_bytes(
                    &instance_url,
                    Some(&signal),
                    "application/dicom; transfer-syntax=*",
                )
                .await?;
                viewer.feed_dicom_slice(slice_index, &bytes)?;
                viewer.render()?;
                self.loaded += 1;
            }

            self.abort_controller = None;
            Ok(())
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn fetch_json(
    url: &str,
    signal: Option<&web_sys::AbortSignal>,
    accept: &str,
) -> Result<JsValue, JsValue> {
    let response = fetch_response(url, signal, accept).await?;
    JsFuture::from(
        response
            .json()
            .map_err(|_| js_error("failed to decode JSON response"))?,
    )
    .await
    .map_err(|error| js_error(format!("failed to await JSON response: {error:?}")))
}

#[cfg(target_arch = "wasm32")]
async fn fetch_bytes(
    url: &str,
    signal: Option<&web_sys::AbortSignal>,
    accept: &str,
) -> Result<Vec<u8>, JsValue> {
    let response = fetch_response(url, signal, accept).await?;
    let buffer = JsFuture::from(
        response
            .array_buffer()
            .map_err(|_| js_error("failed to read response body"))?,
    )
    .await
    .map_err(|error| js_error(format!("failed to await response buffer: {error:?}")))?;
    Ok(Uint8Array::new(&buffer).to_vec())
}

#[cfg(target_arch = "wasm32")]
async fn fetch_response(
    url: &str,
    signal: Option<&web_sys::AbortSignal>,
    accept: &str,
) -> Result<web_sys::Response, JsValue> {
    let init = web_sys::RequestInit::new();
    init.set_method("GET");
    init.set_mode(web_sys::RequestMode::Cors);
    if let Some(signal) = signal {
        init.set_signal(Some(signal));
    }
    let request = web_sys::Request::new_with_str_and_init(url, &init)
        .map_err(|error| js_error(format!("failed to build request: {error:?}")))?;
    request
        .headers()
        .set("Accept", accept)
        .map_err(|_| js_error("failed to set Accept header"))?;
    let window = web_sys::window().ok_or_else(|| js_error("window is not available"))?;
    let response_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|error| js_error(format!("fetch failed: {error:?}")))?;
    let response: web_sys::Response = response_value
        .dyn_into()
        .map_err(|_| js_error("fetch did not return a Response object"))?;
    if !response.ok() {
        return Err(js_error(format!(
            "HTTP {} while fetching {}",
            response.status(),
            url
        )));
    }
    Ok(response)
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
struct InstanceMetadata {
    sop_instance_uid: String,
    instance_number: i32,
    rows: u16,
    columns: u16,
    pixel_spacing: Option<(f64, f64)>,
    slice_thickness: Option<f64>,
    image_position: Option<DVec3>,
    image_orientation: Option<(DVec3, DVec3)>,
    number_of_frames: u32,
}

#[cfg(target_arch = "wasm32")]
fn trim_root(root: &str) -> &str {
    root.trim_end_matches('/')
}

#[cfg(target_arch = "wasm32")]
fn derive_geometry(
    instances: &[InstanceMetadata],
) -> Result<dicomview_core::VolumeGeometry, JsValue> {
    let first = instances
        .first()
        .ok_or_else(|| js_error("series metadata is empty"))?;
    if instances
        .iter()
        .any(|instance| instance.number_of_frames != 1)
    {
        return Err(js_error(
            "multi-frame DICOMweb metadata is not yet supported by the built-in WADO loader",
        ));
    }
    if instances
        .iter()
        .any(|instance| instance.rows != first.rows || instance.columns != first.columns)
    {
        return Err(js_error("series contains inconsistent frame dimensions"));
    }

    let direction = first
        .image_orientation
        .map(|(row, col)| {
            let normal = row.cross(col).normalize_or_zero();
            if normal.length_squared() > 0.0 {
                DMat3::from_cols(row, col, normal)
            } else {
                DMat3::IDENTITY
            }
        })
        .unwrap_or(DMat3::IDENTITY);
    let origin = first.image_position.unwrap_or(DVec3::ZERO);
    let pixel_spacing = first.pixel_spacing.unwrap_or((1.0, 1.0));
    let slice_spacing = if instances.len() >= 2 {
        projected_slice_spacing(instances)
            .or(first.slice_thickness)
            .unwrap_or(1.0)
    } else {
        first.slice_thickness.unwrap_or(1.0)
    };

    Ok(dicomview_core::VolumeGeometry::new(
        UVec3::new(
            first.columns as u32,
            first.rows as u32,
            instances.len() as u32,
        ),
        DVec3::new(pixel_spacing.1, pixel_spacing.0, slice_spacing),
        origin,
        direction,
    ))
}

#[cfg(target_arch = "wasm32")]
fn projected_slice_spacing(instances: &[InstanceMetadata]) -> Option<f64> {
    let first = instances.first()?;
    let second = instances.get(1)?;
    let (row, col) = first.image_orientation?;
    let normal = row.cross(col).normalize_or_zero();
    let first_pos = first.image_position?;
    let second_pos = second.image_position?;
    Some(normal.dot(second_pos - first_pos).abs())
}

#[cfg(target_arch = "wasm32")]
fn parse_series_metadata(metadata_json: &JsValue) -> Result<Vec<InstanceMetadata>, JsValue> {
    let array: Array = metadata_json
        .clone()
        .dyn_into()
        .map_err(|_| js_error("metadata response must be a DICOM JSON array"))?;
    let mut instances = Vec::with_capacity(array.length() as usize);
    for entry in array.iter() {
        instances.push(parse_instance_metadata(&entry)?);
    }
    instances.sort_by(compare_instances);
    Ok(instances)
}

#[cfg(target_arch = "wasm32")]
fn parse_instance_metadata(value: &JsValue) -> Result<InstanceMetadata, JsValue> {
    Ok(InstanceMetadata {
        sop_instance_uid: tag_first_string(value, "00080018")?
            .ok_or_else(|| js_error("metadata entry is missing SOP Instance UID"))?,
        instance_number: tag_first_i32(value, "00200013")?.unwrap_or_default(),
        rows: tag_first_u16(value, "00280010")?
            .ok_or_else(|| js_error("metadata entry is missing Rows"))?,
        columns: tag_first_u16(value, "00280011")?
            .ok_or_else(|| js_error("metadata entry is missing Columns"))?,
        pixel_spacing: tag_pair_f64(value, "00280030")?,
        slice_thickness: tag_first_f64(value, "00180050")?,
        image_position: tag_triplet_f64(value, "00200032")?
            .map(|values| DVec3::new(values[0], values[1], values[2])),
        image_orientation: tag_six_f64(value, "00200037")?.map(|values| {
            (
                DVec3::new(values[0], values[1], values[2]),
                DVec3::new(values[3], values[4], values[5]),
            )
        }),
        number_of_frames: tag_first_u32(value, "00280008")?.unwrap_or(1),
    })
}

#[cfg(target_arch = "wasm32")]
fn compare_instances(left: &InstanceMetadata, right: &InstanceMetadata) -> std::cmp::Ordering {
    match (
        left.image_position,
        right.image_position,
        left.image_orientation,
        right.image_orientation,
    ) {
        (Some(left_position), Some(right_position), Some((row, col)), Some(_)) => {
            let normal = row.cross(col).normalize_or_zero();
            let left_distance = normal.dot(left_position);
            let right_distance = normal.dot(right_position);
            left_distance
                .partial_cmp(&right_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        }
        _ => left
            .instance_number
            .cmp(&right.instance_number)
            .then_with(|| left.sop_instance_uid.cmp(&right.sop_instance_uid)),
    }
}

#[cfg(target_arch = "wasm32")]
fn tag_values(value: &JsValue, tag: &str) -> Result<Option<Array>, JsValue> {
    let entry = Reflect::get(value, &JsValue::from_str(tag))
        .map_err(|_| js_error(format!("failed to read DICOM JSON tag {tag}")))?;
    if entry.is_undefined() || entry.is_null() {
        return Ok(None);
    }
    let values = Reflect::get(&entry, &JsValue::from_str("Value"))
        .map_err(|_| js_error(format!("failed to read Value for tag {tag}")))?;
    if values.is_undefined() || values.is_null() {
        return Ok(None);
    }
    Ok(Some(values.dyn_into().map_err(|_| {
        js_error(format!("tag {tag} Value is not an array"))
    })?))
}

#[cfg(target_arch = "wasm32")]
fn tag_first_string(value: &JsValue, tag: &str) -> Result<Option<String>, JsValue> {
    let values = match tag_values(value, tag)? {
        Some(values) => values,
        None => return Ok(None),
    };
    Ok(values.get(0).as_string())
}

#[cfg(target_arch = "wasm32")]
fn tag_first_f64(value: &JsValue, tag: &str) -> Result<Option<f64>, JsValue> {
    let values = match tag_values(value, tag)? {
        Some(values) => values,
        None => return Ok(None),
    };
    if values.length() == 0 {
        return Ok(None);
    }
    js_value_to_f64(&values.get(0)).map(Some)
}

#[cfg(target_arch = "wasm32")]
fn tag_first_i32(value: &JsValue, tag: &str) -> Result<Option<i32>, JsValue> {
    Ok(tag_first_f64(value, tag)?.map(|value| value as i32))
}

#[cfg(target_arch = "wasm32")]
fn tag_first_u16(value: &JsValue, tag: &str) -> Result<Option<u16>, JsValue> {
    Ok(tag_first_f64(value, tag)?.map(|value| value as u16))
}

#[cfg(target_arch = "wasm32")]
fn tag_first_u32(value: &JsValue, tag: &str) -> Result<Option<u32>, JsValue> {
    Ok(tag_first_f64(value, tag)?.map(|value| value as u32))
}

#[cfg(target_arch = "wasm32")]
fn tag_pair_f64(value: &JsValue, tag: &str) -> Result<Option<(f64, f64)>, JsValue> {
    let values = match tag_values(value, tag)? {
        Some(values) => values,
        None => return Ok(None),
    };
    if values.length() < 2 {
        return Ok(None);
    }
    Ok(Some((
        js_value_to_f64(&values.get(0))?,
        js_value_to_f64(&values.get(1))?,
    )))
}

#[cfg(target_arch = "wasm32")]
fn tag_triplet_f64(value: &JsValue, tag: &str) -> Result<Option<[f64; 3]>, JsValue> {
    let values = match tag_values(value, tag)? {
        Some(values) => values,
        None => return Ok(None),
    };
    if values.length() < 3 {
        return Ok(None);
    }
    Ok(Some([
        js_value_to_f64(&values.get(0))?,
        js_value_to_f64(&values.get(1))?,
        js_value_to_f64(&values.get(2))?,
    ]))
}

#[cfg(target_arch = "wasm32")]
fn tag_six_f64(value: &JsValue, tag: &str) -> Result<Option<[f64; 6]>, JsValue> {
    let values = match tag_values(value, tag)? {
        Some(values) => values,
        None => return Ok(None),
    };
    if values.length() < 6 {
        return Ok(None);
    }
    Ok(Some([
        js_value_to_f64(&values.get(0))?,
        js_value_to_f64(&values.get(1))?,
        js_value_to_f64(&values.get(2))?,
        js_value_to_f64(&values.get(3))?,
        js_value_to_f64(&values.get(4))?,
        js_value_to_f64(&values.get(5))?,
    ]))
}

#[cfg(target_arch = "wasm32")]
fn js_value_to_f64(value: &JsValue) -> Result<f64, JsValue> {
    if let Some(number) = value.as_f64() {
        return Ok(number);
    }
    if let Some(text) = value.as_string() {
        return text
            .parse::<f64>()
            .map_err(|_| js_error(format!("failed to parse numeric DICOM JSON value `{text}`")));
    }
    Err(js_error(
        "DICOM JSON numeric value must be a number or string",
    ))
}
