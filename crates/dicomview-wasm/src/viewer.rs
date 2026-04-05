//! Browser-facing viewer facade.

use crate::utils::js_error;
use dicomview_core::{
    decode_dicom, SlicePreviewMode, SliceProjectionMode, VolumeBlendMode, VolumeGeometry,
    VolumePresetId,
};
use glam::{DMat3, DVec3, UVec3};
use js_sys::Reflect;
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use dicomview_gpu::{
    CanvasSurface, FrameTargets, RenderEngine, RenderTarget, SingleSliceEngine, Viewport,
};
#[cfg(target_arch = "wasm32")]
use std::sync::Arc;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

/// One JS-visible viewer instance managing four canvases.
#[wasm_bindgen]
pub struct Viewer {
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    inner: ViewerInner,
}

#[cfg(target_arch = "wasm32")]
struct ViewerInner {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    engine: RenderEngine,
    axial: CanvasBinding,
    coronal: CanvasBinding,
    sagittal: CanvasBinding,
    volume: CanvasBinding,
}

#[cfg(target_arch = "wasm32")]
struct CanvasBinding {
    canvas: web_sys::HtmlCanvasElement,
    surface: CanvasSurface,
}

#[cfg(not(target_arch = "wasm32"))]
struct ViewerInner;

#[wasm_bindgen]
impl Viewer {
    /// Creates a new viewer bound to four canvas elements.
    #[wasm_bindgen(js_name = create)]
    pub async fn create(config: JsValue) -> Result<Viewer, JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = config;
            Err(js_error(
                "dicomview-wasm Viewer is only supported on wasm32 targets",
            ))
        }

        #[cfg(target_arch = "wasm32")]
        {
            let axial_canvas = extract_canvas(&config, "axial")?;
            let coronal_canvas = extract_canvas(&config, "coronal")?;
            let sagittal_canvas = extract_canvas(&config, "sagittal")?;
            let volume_canvas = extract_canvas(&config, "volume")?;

            let instance = wgpu::Instance::default();
            let adapter: wgpu::Adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                })
                .await
                .map_err(|error| js_error(format!("failed to acquire GPU adapter: {error}")))?;
            let (device, queue): (wgpu::Device, wgpu::Queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .map_err(|error| js_error(format!("failed to acquire GPU device: {error}")))?;
            let device = Arc::new(device);
            let queue = Arc::new(queue);

            let axial_surface = CanvasSurface::from_canvas(
                &instance,
                &adapter,
                &device,
                axial_canvas.clone(),
                None,
            )
            .map_err(|error| js_error(error.to_string()))?;
            let output_format = axial_surface.format;
            let coronal_surface = CanvasSurface::from_canvas(
                &instance,
                &adapter,
                &device,
                coronal_canvas.clone(),
                Some(output_format),
            )
            .map_err(|error| js_error(error.to_string()))?;
            let sagittal_surface = CanvasSurface::from_canvas(
                &instance,
                &adapter,
                &device,
                sagittal_canvas.clone(),
                Some(output_format),
            )
            .map_err(|error| js_error(error.to_string()))?;
            let volume_surface = CanvasSurface::from_canvas(
                &instance,
                &adapter,
                &device,
                volume_canvas.clone(),
                Some(output_format),
            )
            .map_err(|error| js_error(error.to_string()))?;
            let engine = RenderEngine::from_arc(device.clone(), queue.clone(), output_format);

            Ok(Self {
                inner: ViewerInner {
                    device,
                    queue,
                    engine,
                    axial: CanvasBinding {
                        canvas: axial_canvas,
                        surface: axial_surface,
                    },
                    coronal: CanvasBinding {
                        canvas: coronal_canvas,
                        surface: coronal_surface,
                    },
                    sagittal: CanvasBinding {
                        canvas: sagittal_canvas,
                        surface: sagittal_surface,
                    },
                    volume: CanvasBinding {
                        canvas: volume_canvas,
                        surface: volume_surface,
                    },
                },
            })
        }
    }

    /// Prepares an empty volume with the provided geometry object.
    pub fn prepare_volume(&mut self, geometry: JsValue) -> Result<(), JsValue> {
        let geometry = parse_geometry(&geometry)?;
        self.prepare_volume_native(geometry)
    }

    /// Decodes one DICOM Part 10 payload and uploads its frame data.
    pub fn feed_dicom_slice(&mut self, z_index: usize, bytes: &[u8]) -> Result<(), JsValue> {
        let frames = decode_dicom(bytes).map_err(|error| js_error(error.to_string()))?;
        for (frame_offset, frame) in frames.into_iter().enumerate() {
            self.feed_pixel_slice_native((z_index + frame_offset) as u32, &frame.pixels)?;
        }
        Ok(())
    }

    /// Uploads one already-decoded signed 16-bit slice.
    pub fn feed_pixel_slice(&mut self, z_index: usize, pixels: &[i16]) -> Result<(), JsValue> {
        self.feed_pixel_slice_native(z_index as u32, pixels)
    }

    /// Returns the current loading progress in `[0, 1]`.
    #[must_use]
    pub fn loading_progress(&self) -> f64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            0.0
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner
                .engine
                .prepared_volume()
                .map_or(0.0, |volume| volume.loading_progress())
        }
    }

    /// Renders all four canvases.
    pub fn render(&mut self) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Err(js_error("rendering is only supported on wasm32 targets"))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner.render()
        }
    }

    /// Updates the shared MPR crosshair in world coordinates.
    pub fn set_crosshair(&mut self, x: f64, y: f64, z: f64) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (x, y, z);
            Err(js_error(
                "crosshair updates are only supported on wasm32 targets",
            ))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner
                .engine
                .set_crosshair(DVec3::new(x, y, z))
                .map_err(|error| js_error(error.to_string()))
        }
    }

    /// Scrolls one of the three slice viewports.
    pub fn scroll_slice(&mut self, viewport: u8, delta: f64) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (viewport, delta);
            Err(js_error(
                "slice scrolling is only supported on wasm32 targets",
            ))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner
                .engine
                .scroll_slice(parse_slice_viewport(viewport)?, delta)
                .map_err(|error| js_error(error.to_string()))
        }
    }

    /// Applies one window/level setting to all viewports.
    pub fn set_window_level(&mut self, center: f64, width: f64) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (center, width);
            Err(js_error(
                "window/level updates are only supported on wasm32 targets",
            ))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner
                .engine
                .set_window_level(center, width)
                .map_err(|error| js_error(error.to_string()))
        }
    }

    /// Orbits the 3D volume camera.
    pub fn orbit(&mut self, dx: f64, dy: f64) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (dx, dy);
            Err(js_error("camera orbit is only supported on wasm32 targets"))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner.engine.volume_state_mut().orbit(dx, dy);
            Ok(())
        }
    }

    /// Pans the 3D volume camera.
    pub fn pan(&mut self, dx: f64, dy: f64) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (dx, dy);
            Err(js_error("camera pan is only supported on wasm32 targets"))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner.engine.volume_state_mut().pan(dx, dy);
            Ok(())
        }
    }

    /// Zooms the 3D volume camera.
    pub fn zoom(&mut self, factor: f64) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = factor;
            Err(js_error("camera zoom is only supported on wasm32 targets"))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner.engine.volume_state_mut().zoom_by(factor);
            Ok(())
        }
    }

    /// Selects the active volume blend mode.
    pub fn set_blend_mode(&mut self, mode: u8) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = mode;
            Err(js_error(
                "blend-mode updates are only supported on wasm32 targets",
            ))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner.engine.volume_state_mut().blend_mode = parse_blend_mode(mode)?;
            Ok(())
        }
    }

    /// Configures thick-slab rendering for one slice viewport.
    pub fn set_thick_slab(
        &mut self,
        viewport: u8,
        thickness: f64,
        projection: u8,
    ) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (viewport, thickness, projection);
            Err(js_error(
                "thick-slab updates are only supported on wasm32 targets",
            ))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner.engine.set_thick_slab(
                parse_slice_viewport(viewport)?,
                thickness,
                parse_projection_mode(projection)?,
            );
            Ok(())
        }
    }

    /// Switches to one of the built-in volume presets.
    pub fn set_volume_preset(&mut self, name: &str) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = name;
            Err(js_error(
                "preset updates are only supported on wasm32 targets",
            ))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner.engine.set_volume_preset(parse_preset_id(name)?);
            Ok(())
        }
    }

    /// Resets all viewport state back to defaults.
    pub fn reset(&mut self) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Err(js_error("reset is only supported on wasm32 targets"))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner.engine.reset();
            Ok(())
        }
    }

    /// Explicitly destroys the viewer and releases its resources.
    pub fn destroy(self) {}
}

impl Viewer {
    pub(crate) fn prepare_volume_native(
        &mut self,
        geometry: VolumeGeometry,
    ) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = geometry;
            Err(js_error(
                "volume preparation is only supported on wasm32 targets",
            ))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner
                .engine
                .prepare_volume(geometry)
                .map_err(|error| js_error(error.to_string()))
        }
    }

    pub(crate) fn feed_pixel_slice_native(
        &mut self,
        z_index: u32,
        pixels: &[i16],
    ) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (z_index, pixels);
            Err(js_error(
                "pixel uploads are only supported on wasm32 targets",
            ))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner
                .engine
                .insert_slice(z_index, pixels)
                .map_err(|error| js_error(error.to_string()))
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl ViewerInner {
    fn render(&mut self) -> Result<(), JsValue> {
        let device = self.device.clone();
        let axial = acquire_frame(&mut self.axial, &device)?;
        let coronal = acquire_frame(&mut self.coronal, &device)?;
        let sagittal = acquire_frame(&mut self.sagittal, &device)?;
        let volume = acquire_frame(&mut self.volume, &device)?;

        let axial_view = axial
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let coronal_view = coronal
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let sagittal_view = sagittal
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let volume_view = volume
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("dicomview-frame"),
            });
        self.engine
            .render_frame(
                &mut encoder,
                FrameTargets {
                    axial: RenderTarget {
                        view: &axial_view,
                        viewport: viewport_from_binding(&self.axial),
                    },
                    coronal: RenderTarget {
                        view: &coronal_view,
                        viewport: viewport_from_binding(&self.coronal),
                    },
                    sagittal: RenderTarget {
                        view: &sagittal_view,
                        viewport: viewport_from_binding(&self.sagittal),
                    },
                    volume: RenderTarget {
                        view: &volume_view,
                        viewport: viewport_from_binding(&self.volume),
                    },
                },
                true,
            )
            .map_err(|error| js_error(error.to_string()))?;
        self.queue.submit(Some(encoder.finish()));

        axial.present();
        coronal.present();
        sagittal.present();
        volume.present();
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
fn acquire_frame(
    binding: &mut CanvasBinding,
    device: &wgpu::Device,
) -> Result<wgpu::SurfaceTexture, JsValue> {
    let width = binding.canvas.width().max(1);
    let height = binding.canvas.height().max(1);
    if binding.surface.size != (width, height) {
        binding.surface.resize(device, width, height);
    }

    match binding.surface.surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(frame)
        | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => Ok(frame),
        wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
            binding.surface.resize(device, width, height);
            match binding.surface.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(frame)
                | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => Ok(frame),
                wgpu::CurrentSurfaceTexture::Timeout => {
                    Err(js_error("failed to reacquire surface texture: timeout"))
                }
                wgpu::CurrentSurfaceTexture::Occluded => Err(js_error(
                    "failed to reacquire surface texture: surface occluded",
                )),
                wgpu::CurrentSurfaceTexture::Outdated => Err(js_error(
                    "failed to reacquire surface texture: surface is outdated after reconfigure",
                )),
                wgpu::CurrentSurfaceTexture::Lost => Err(js_error(
                    "failed to reacquire surface texture: surface was lost after reconfigure",
                )),
                wgpu::CurrentSurfaceTexture::Validation => Err(js_error(
                    "failed to reacquire surface texture: validation error",
                )),
            }
        }
        wgpu::CurrentSurfaceTexture::Timeout => {
            Err(js_error("failed to acquire surface texture: timeout"))
        }
        wgpu::CurrentSurfaceTexture::Occluded => Err(js_error(
            "failed to acquire surface texture: surface occluded",
        )),
        wgpu::CurrentSurfaceTexture::Validation => Err(js_error(
            "failed to acquire surface texture: validation error",
        )),
    }
}

#[cfg(target_arch = "wasm32")]
fn viewport_from_binding(binding: &CanvasBinding) -> Viewport {
    Viewport {
        x: 0,
        y: 0,
        width: binding.surface.size.0,
        height: binding.surface.size.1,
    }
}

#[cfg(target_arch = "wasm32")]
fn extract_canvas(config: &JsValue, key: &str) -> Result<web_sys::HtmlCanvasElement, JsValue> {
    Reflect::get(config, &JsValue::from_str(key))
        .map_err(|_| js_error(format!("missing `{key}` canvas in viewer config")))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| js_error(format!("`{key}` must be an HTMLCanvasElement")))
}

fn parse_geometry(value: &JsValue) -> Result<VolumeGeometry, JsValue> {
    let dimensions = get_js_value(value, "dimensions")?;
    let spacing = get_js_value(value, "spacing")?;
    let origin = get_js_value(value, "origin")?;
    let direction = get_js_value(value, "direction")?;

    let dims = read_u32_triplet(&dimensions)?;
    let spacing = read_f64_triplet(&spacing)?;
    let origin = read_f64_triplet(&origin)?;
    let direction_cols = read_f64_matrix3(&direction)?;

    Ok(VolumeGeometry::new(
        UVec3::new(dims[0], dims[1], dims[2]),
        DVec3::new(spacing[0], spacing[1], spacing[2]),
        DVec3::new(origin[0], origin[1], origin[2]),
        DMat3::from_cols(
            DVec3::new(
                direction_cols[0][0],
                direction_cols[0][1],
                direction_cols[0][2],
            ),
            DVec3::new(
                direction_cols[1][0],
                direction_cols[1][1],
                direction_cols[1][2],
            ),
            DVec3::new(
                direction_cols[2][0],
                direction_cols[2][1],
                direction_cols[2][2],
            ),
        ),
    ))
}

fn get_js_value(value: &JsValue, key: &str) -> Result<JsValue, JsValue> {
    Reflect::get(value, &JsValue::from_str(key))
        .map_err(|_| js_error(format!("missing `{key}` field")))
        .and_then(|field| {
            if field.is_undefined() || field.is_null() {
                Err(js_error(format!("missing `{key}` field")))
            } else {
                Ok(field)
            }
        })
}

fn read_u32_triplet(value: &JsValue) -> Result<[u32; 3], JsValue> {
    Ok([
        read_array_entry_u32(value, 0)?,
        read_array_entry_u32(value, 1)?,
        read_array_entry_u32(value, 2)?,
    ])
}

fn read_f64_triplet(value: &JsValue) -> Result<[f64; 3], JsValue> {
    Ok([
        read_array_entry_f64(value, 0)?,
        read_array_entry_f64(value, 1)?,
        read_array_entry_f64(value, 2)?,
    ])
}

fn read_f64_matrix3(value: &JsValue) -> Result<[[f64; 3]; 3], JsValue> {
    Ok([
        read_f64_triplet(
            &Reflect::get(value, &JsValue::from_f64(0.0))
                .map_err(|_| js_error("invalid direction matrix"))?,
        )?,
        read_f64_triplet(
            &Reflect::get(value, &JsValue::from_f64(1.0))
                .map_err(|_| js_error("invalid direction matrix"))?,
        )?,
        read_f64_triplet(
            &Reflect::get(value, &JsValue::from_f64(2.0))
                .map_err(|_| js_error("invalid direction matrix"))?,
        )?,
    ])
}

fn read_array_entry_f64(value: &JsValue, index: u32) -> Result<f64, JsValue> {
    let entry = Reflect::get(value, &JsValue::from_f64(f64::from(index)))
        .map_err(|_| js_error("invalid numeric array"))?;
    entry
        .as_f64()
        .ok_or_else(|| js_error("array entry must be numeric"))
}

fn read_array_entry_u32(value: &JsValue, index: u32) -> Result<u32, JsValue> {
    let entry = Reflect::get(value, &JsValue::from_f64(f64::from(index)))
        .map_err(|_| js_error("invalid integer array"))?;
    let number = entry
        .as_f64()
        .ok_or_else(|| js_error("array entry must be numeric"))?;
    if !(0.0..=u32::MAX as f64).contains(&number) {
        return Err(js_error("integer array entry is out of range"));
    }
    Ok(number as u32)
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn parse_slice_viewport(value: u8) -> Result<SlicePreviewMode, JsValue> {
    match value {
        0 => Ok(SlicePreviewMode::Axial),
        1 => Ok(SlicePreviewMode::Coronal),
        2 => Ok(SlicePreviewMode::Sagittal),
        _ => Err(js_error(
            "viewport must be 0=axial, 1=coronal, or 2=sagittal",
        )),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn parse_projection_mode(value: u8) -> Result<SliceProjectionMode, JsValue> {
    match value {
        0 => Ok(SliceProjectionMode::Thin),
        1 => Ok(SliceProjectionMode::MaximumIntensity),
        2 => Ok(SliceProjectionMode::MinimumIntensity),
        3 => Ok(SliceProjectionMode::AverageIntensity),
        _ => Err(js_error(
            "projection must be 0=thin, 1=mip, 2=minip, or 3=average",
        )),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn parse_blend_mode(value: u8) -> Result<VolumeBlendMode, JsValue> {
    match value {
        0 => Ok(VolumeBlendMode::Composite),
        1 => Ok(VolumeBlendMode::MaximumIntensity),
        2 => Ok(VolumeBlendMode::MinimumIntensity),
        3 => Ok(VolumeBlendMode::AverageIntensity),
        _ => Err(js_error(
            "blend mode must be 0=composite, 1=mip, 2=minip, or 3=average",
        )),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn parse_preset_id(name: &str) -> Result<VolumePresetId, JsValue> {
    let normalized: String = name.to_lowercase().replace(' ', "");
    match normalized.as_str() {
        "ct-bone" | "ctbone" => Ok(VolumePresetId::CtBone),
        "ct-soft-tissue" | "ctsofttissue" => Ok(VolumePresetId::CtSoftTissue),
        "ct-lung" | "ctlung" => Ok(VolumePresetId::CtLung),
        "ct-mip" | "ctmip" => Ok(VolumePresetId::CtMip),
        "mr-default" | "mrdefault" => Ok(VolumePresetId::MrDefault),
        "mr-angio" | "mrangio" => Ok(VolumePresetId::MrAngio),
        "mr-t2-brain" | "mrt2brain" | "mr-t2brain" => Ok(VolumePresetId::MrT2Brain),
        _ => Err(js_error(format!("unknown volume preset `{name}`"))),
    }
}

// ---------------------------------------------------------------------------
// StackViewer — single-canvas viewer for 2D stack browsing
// ---------------------------------------------------------------------------

/// A single-canvas viewer for 2D stack browsing (like cornerstone's `StackViewport`).
///
/// Unlike [`Viewer`] which requires 4 canvases, this only needs one.
#[wasm_bindgen]
pub struct StackViewer {
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    inner: StackViewerInner,
}

#[cfg(target_arch = "wasm32")]
struct StackViewerInner {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    engine: SingleSliceEngine,
    canvas: CanvasBinding,
}

#[cfg(not(target_arch = "wasm32"))]
struct StackViewerInner;

#[wasm_bindgen]
impl StackViewer {
    /// Creates a new stack viewer bound to a single canvas element.
    #[wasm_bindgen(js_name = create)]
    pub async fn create(config: JsValue) -> Result<StackViewer, JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = config;
            Err(js_error(
                "dicomview-wasm StackViewer is only supported on wasm32 targets",
            ))
        }

        #[cfg(target_arch = "wasm32")]
        {
            let canvas = extract_canvas(&config, "canvas")?;

            let instance = wgpu::Instance::default();
            let adapter: wgpu::Adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                })
                .await
                .map_err(|error| js_error(format!("failed to acquire GPU adapter: {error}")))?;
            let (device, queue): (wgpu::Device, wgpu::Queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .map_err(|error| js_error(format!("failed to acquire GPU device: {error}")))?;
            let device = Arc::new(device);
            let queue = Arc::new(queue);

            let surface = CanvasSurface::from_canvas(
                &instance,
                &adapter,
                &device,
                canvas.clone(),
                None,
            )
            .map_err(|error| js_error(error.to_string()))?;
            let output_format = surface.format;
            let engine = SingleSliceEngine::from_arc(device.clone(), queue.clone(), output_format);

            Ok(Self {
                inner: StackViewerInner {
                    device,
                    queue,
                    engine,
                    canvas: CanvasBinding { canvas, surface },
                },
            })
        }
    }

    /// Prepares an empty volume with the provided geometry object.
    pub fn prepare_volume(&mut self, geometry: JsValue) -> Result<(), JsValue> {
        let geometry = parse_geometry(&geometry)?;
        self.prepare_volume_native(geometry)
    }

    /// Decodes one DICOM Part 10 payload and uploads its frame data.
    pub fn feed_dicom_slice(&mut self, z_index: usize, bytes: &[u8]) -> Result<(), JsValue> {
        let frames = decode_dicom(bytes).map_err(|error| js_error(error.to_string()))?;
        for (frame_offset, frame) in frames.into_iter().enumerate() {
            self.feed_pixel_slice_native((z_index + frame_offset) as u32, &frame.pixels)?;
        }
        Ok(())
    }

    /// Uploads one already-decoded signed 16-bit slice.
    pub fn feed_pixel_slice(&mut self, z_index: usize, pixels: &[i16]) -> Result<(), JsValue> {
        self.feed_pixel_slice_native(z_index as u32, pixels)
    }

    /// Returns the current loading progress in `[0, 1]`.
    #[must_use]
    pub fn loading_progress(&self) -> f64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            0.0
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner
                .engine
                .prepared_volume()
                .map_or(0.0, |volume| volume.loading_progress())
        }
    }

    /// Renders the single slice canvas.
    pub fn render(&mut self) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Err(js_error("rendering is only supported on wasm32 targets"))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner.render()
        }
    }

    /// Scrolls the slice along its normal.
    pub fn scroll_slice(&mut self, delta: f64) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = delta;
            Err(js_error(
                "slice scrolling is only supported on wasm32 targets",
            ))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner
                .engine
                .scroll_slice(delta)
                .map_err(|error| js_error(error.to_string()))
        }
    }

    /// Applies one window/level setting to the viewport.
    pub fn set_window_level(&mut self, center: f64, width: f64) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (center, width);
            Err(js_error(
                "window/level updates are only supported on wasm32 targets",
            ))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner
                .engine
                .set_window_level(center, width)
                .map_err(|error| js_error(error.to_string()))
        }
    }

    /// Switches which orthogonal plane is displayed.
    pub fn set_slice_mode(&mut self, viewport: u8) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = viewport;
            Err(js_error(
                "slice mode updates are only supported on wasm32 targets",
            ))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner
                .engine
                .set_slice_mode(parse_slice_viewport(viewport)?);
            Ok(())
        }
    }

    /// Resets all viewport state back to defaults.
    pub fn reset(&mut self) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Err(js_error("reset is only supported on wasm32 targets"))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner.engine.reset();
            Ok(())
        }
    }

    /// Explicitly destroys the stack viewer and releases its resources.
    pub fn destroy(self) {}
}

impl StackViewer {
    fn prepare_volume_native(&mut self, geometry: VolumeGeometry) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = geometry;
            Err(js_error(
                "volume preparation is only supported on wasm32 targets",
            ))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner
                .engine
                .prepare_volume(geometry)
                .map_err(|error| js_error(error.to_string()))
        }
    }

    fn feed_pixel_slice_native(&mut self, z_index: u32, pixels: &[i16]) -> Result<(), JsValue> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (z_index, pixels);
            Err(js_error(
                "pixel uploads are only supported on wasm32 targets",
            ))
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.inner
                .engine
                .insert_slice(z_index, pixels)
                .map_err(|error| js_error(error.to_string()))
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl StackViewerInner {
    fn render(&mut self) -> Result<(), JsValue> {
        let device = self.device.clone();
        let frame = acquire_frame(&mut self.canvas, &device)?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("dicomview-stack-frame"),
            });
        self.engine
            .render_slice(
                &mut encoder,
                &RenderTarget {
                    view: &view,
                    viewport: viewport_from_binding(&self.canvas),
                },
            )
            .map_err(|error| js_error(error.to_string()))?;
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_slice_viewports() {
        assert_eq!(parse_slice_viewport(0).unwrap(), SlicePreviewMode::Axial);
        assert_eq!(parse_slice_viewport(1).unwrap(), SlicePreviewMode::Coronal);
        assert_eq!(parse_slice_viewport(2).unwrap(), SlicePreviewMode::Sagittal);
    }

    #[test]
    fn parses_preset_names() {
        assert_eq!(parse_preset_id("ct-bone").unwrap(), VolumePresetId::CtBone);
        assert!(parse_preset_id("unknown").is_err());
    }

    #[test]
    fn parses_cornerstone_style_preset_names() {
        assert_eq!(parse_preset_id("CT-Bone").unwrap(), VolumePresetId::CtBone);
        assert_eq!(
            parse_preset_id("CT-Soft-Tissue").unwrap(),
            VolumePresetId::CtSoftTissue
        );
        assert_eq!(parse_preset_id("CT-Lung").unwrap(), VolumePresetId::CtLung);
        assert_eq!(parse_preset_id("CT-MIP").unwrap(), VolumePresetId::CtMip);
        assert_eq!(
            parse_preset_id("MR-Default").unwrap(),
            VolumePresetId::MrDefault
        );
        assert_eq!(
            parse_preset_id("MR-Angio").unwrap(),
            VolumePresetId::MrAngio
        );
        assert_eq!(
            parse_preset_id("MR-T2-Brain").unwrap(),
            VolumePresetId::MrT2Brain
        );
    }

    #[test]
    fn parses_mixed_case_preset_names() {
        assert_eq!(
            parse_preset_id("ct-BONE").unwrap(),
            VolumePresetId::CtBone
        );
        assert_eq!(
            parse_preset_id("Ct-Soft-Tissue").unwrap(),
            VolumePresetId::CtSoftTissue
        );
    }
}
