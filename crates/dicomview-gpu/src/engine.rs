//! Multi-viewport orchestration on top of `volren-gpu`.

use crate::incremental_texture::update_texture_slice_i16;
use dicomview_core::{
    preset, IncrementalVolume, IncrementalVolumeError, SlicePreviewMode, SlicePreviewState,
    SliceProjectionMode, VolumeBlendMode, VolumeGeometry, VolumePresetId, VolumeViewState,
};
use glam::DVec3;
use std::sync::Arc;
use thiserror::Error;
use volren_core::{
    camera::{Camera, Projection},
    render_params::{BlendMode, VolumeRenderParams},
    transfer_function::{ColorTransferFunction, OpacityTransferFunction},
    Aabb, WindowLevel,
};
use volren_gpu::{CrosshairParams, RenderError, Viewport, VolumeRenderer};

/// One render target view paired with its viewport rectangle.
pub struct RenderTarget<'a> {
    /// The output texture view to render into.
    pub view: &'a wgpu::TextureView,
    /// The sub-viewport inside that texture.
    pub viewport: Viewport,
}

/// The four targets required for one standard MPR + volume frame.
pub struct FrameTargets<'a> {
    /// Axial viewport target.
    pub axial: RenderTarget<'a>,
    /// Coronal viewport target.
    pub coronal: RenderTarget<'a>,
    /// Sagittal viewport target.
    pub sagittal: RenderTarget<'a>,
    /// Volume viewport target.
    pub volume: RenderTarget<'a>,
}

/// Errors raised while preparing or rendering the dicomview GPU layer.
#[derive(Debug, Error)]
pub enum RenderEngineError {
    /// Rendering was requested before a volume was prepared.
    #[error("no prepared volume is available")]
    NoPreparedVolume,
    /// The underlying incremental volume rejected the update.
    #[error(transparent)]
    IncrementalVolume(#[from] IncrementalVolumeError),
    /// The underlying renderer rejected the draw or upload request.
    #[error(transparent)]
    Render(#[from] RenderError),
}

/// Shared renderer and viewport state for the four-canvas layout.
pub struct RenderEngine {
    renderer: VolumeRenderer,
    prepared_volume: Option<IncrementalVolume>,
    geometry: Option<VolumeGeometry>,
    volume_state: VolumeViewState,
    axial_state: SlicePreviewState,
    coronal_state: SlicePreviewState,
    sagittal_state: SlicePreviewState,
    active_preset: VolumePresetId,
    #[allow(dead_code)]
    device: Arc<wgpu::Device>,
    #[allow(dead_code)]
    queue: Arc<wgpu::Queue>,
}

impl RenderEngine {
    /// Creates a renderer that targets the provided output format.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        output_format: wgpu::TextureFormat,
    ) -> Self {
        Self::from_arc(
            Arc::new(device.clone()),
            Arc::new(queue.clone()),
            output_format,
        )
    }

    /// Creates a renderer from shared `Arc` device and queue handles.
    #[must_use]
    pub fn from_arc(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        output_format: wgpu::TextureFormat,
    ) -> Self {
        let mut coronal_state = SlicePreviewState::default();
        coronal_state.set_mode(SlicePreviewMode::Coronal);
        let mut sagittal_state = SlicePreviewState::default();
        sagittal_state.set_mode(SlicePreviewMode::Sagittal);

        Self {
            renderer: VolumeRenderer::from_arc(device.clone(), queue.clone(), output_format),
            prepared_volume: None,
            geometry: None,
            volume_state: VolumeViewState::default(),
            axial_state: SlicePreviewState::default(),
            coronal_state,
            sagittal_state,
            active_preset: VolumePresetId::CtSoftTissue,
            device,
            queue,
        }
    }

    /// Prepares an empty progressive volume and allocates its GPU texture.
    pub fn prepare_volume(&mut self, geometry: VolumeGeometry) -> Result<(), RenderEngineError> {
        self.prepared_volume = Some(IncrementalVolume::new(geometry)?);
        self.geometry = Some(geometry);
        self.renderer.allocate_volume(
            geometry.dimensions,
            geometry.spacing,
            geometry.origin,
            geometry.direction,
            (0.0, 1.0),
            true,
        );
        Ok(())
    }

    /// Inserts one slice into the progressive volume and uploads it to the GPU.
    pub fn insert_slice(&mut self, z_index: u32, pixels: &[i16]) -> Result<(), RenderEngineError> {
        let volume = self
            .prepared_volume
            .as_mut()
            .ok_or(RenderEngineError::NoPreparedVolume)?;
        volume.insert_slice(z_index, pixels)?;
        let scalar_range = volume
            .scalar_range()
            .map(|(min, max)| (f64::from(min), f64::from(max)))
            .unwrap_or((0.0, 1.0));
        update_texture_slice_i16(&mut self.renderer, z_index, pixels, scalar_range)?;
        Ok(())
    }

    /// Returns the prepared progressive volume, if any.
    #[must_use]
    pub fn prepared_volume(&self) -> Option<&IncrementalVolume> {
        self.prepared_volume.as_ref()
    }

    /// Returns the active volume geometry, if any.
    #[must_use]
    pub fn geometry(&self) -> Option<VolumeGeometry> {
        self.geometry
    }

    /// Returns the currently known scalar range.
    #[must_use]
    pub fn scalar_range(&self) -> Option<(f64, f64)> {
        self.prepared_volume
            .as_ref()
            .and_then(IncrementalVolume::scalar_range)
            .map(|(min, max)| (f64::from(min), f64::from(max)))
    }

    /// Returns mutable access to the volume viewport state.
    pub fn volume_state_mut(&mut self) -> &mut VolumeViewState {
        &mut self.volume_state
    }

    /// Returns mutable access to one slice viewport state.
    pub fn slice_state_mut(&mut self, mode: SlicePreviewMode) -> &mut SlicePreviewState {
        match mode {
            SlicePreviewMode::Axial => &mut self.axial_state,
            SlicePreviewMode::Coronal => &mut self.coronal_state,
            SlicePreviewMode::Sagittal => &mut self.sagittal_state,
        }
    }

    /// Sets the active volume-rendering preset.
    pub fn set_volume_preset(&mut self, preset_id: VolumePresetId) {
        self.active_preset = preset_id;
    }

    /// Moves the shared MPR crosshair and centers all slice views on that point.
    pub fn set_crosshair(&mut self, world: DVec3) -> Result<(), RenderEngineError> {
        let bounds = self.bounds()?;
        for state in [
            &mut self.axial_state,
            &mut self.coronal_state,
            &mut self.sagittal_state,
        ] {
            state.set_crosshair_world(world);
            state.center_on_world(world, bounds);
        }
        Ok(())
    }

    /// Scrolls one slice viewport along its normal.
    pub fn scroll_slice(
        &mut self,
        mode: SlicePreviewMode,
        delta: f64,
    ) -> Result<(), RenderEngineError> {
        let bounds = self.bounds()?;
        self.slice_state_mut(mode).scroll_by(delta, bounds);
        Ok(())
    }

    /// Applies one transfer window to all viewports.
    pub fn set_window_level(&mut self, center: f64, width: f64) -> Result<(), RenderEngineError> {
        let (scalar_min, scalar_max) = self
            .scalar_range()
            .ok_or(RenderEngineError::NoPreparedVolume)?;
        for state in [
            &mut self.axial_state,
            &mut self.coronal_state,
            &mut self.sagittal_state,
        ] {
            state.set_transfer_window(center, width, scalar_min, scalar_max);
        }
        self.volume_state
            .set_transfer_window(center, width, scalar_min, scalar_max);
        Ok(())
    }

    /// Configures the thick-slab mode for one slice viewport.
    pub fn set_thick_slab(
        &mut self,
        mode: SlicePreviewMode,
        thickness: f64,
        projection_mode: SliceProjectionMode,
    ) {
        let state = self.slice_state_mut(mode);
        if thickness <= 0.0 {
            state.projection_mode = SliceProjectionMode::Thin;
            state.slab_half_thickness = 0.0;
        } else {
            state.projection_mode = projection_mode;
            state.slab_half_thickness = (thickness * 0.5).max(0.5);
        }
    }

    /// Resets all viewports back to their default interaction state.
    pub fn reset(&mut self) {
        self.volume_state.reset();
        self.axial_state.reset();
        self.coronal_state.reset();
        self.coronal_state.set_mode(SlicePreviewMode::Coronal);
        self.sagittal_state.reset();
        self.sagittal_state.set_mode(SlicePreviewMode::Sagittal);
    }

    /// Renders the four-view layout into the provided targets.
    pub fn render_frame(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        targets: FrameTargets<'_>,
        show_crosshairs: bool,
    ) -> Result<(), RenderEngineError> {
        let volume = self
            .prepared_volume
            .as_ref()
            .ok_or(RenderEngineError::NoPreparedVolume)?;
        let geometry = self.geometry.ok_or(RenderEngineError::NoPreparedVolume)?;
        let bounds = bounds_from_geometry(geometry);
        let scalar_range = volume
            .scalar_range()
            .map(|(min, max)| (f64::from(min), f64::from(max)))
            .unwrap_or((0.0, 1.0));

        self.render_slice_view(
            encoder,
            &targets.axial,
            &self.axial_state,
            bounds,
            scalar_range,
            show_crosshairs,
            crosshair_colors(SlicePreviewMode::Axial),
        )?;
        self.render_slice_view(
            encoder,
            &targets.coronal,
            &self.coronal_state,
            bounds,
            scalar_range,
            show_crosshairs,
            crosshair_colors(SlicePreviewMode::Coronal),
        )?;
        self.render_slice_view(
            encoder,
            &targets.sagittal,
            &self.sagittal_state,
            bounds,
            scalar_range,
            show_crosshairs,
            crosshair_colors(SlicePreviewMode::Sagittal),
        )?;

        let camera = camera_for_state(geometry, self.volume_state);
        let params = render_params_for_state(self.active_preset, self.volume_state, scalar_range);
        self.renderer.render_volume(
            encoder,
            targets.volume.view,
            &camera,
            &params,
            targets.volume.viewport,
        )?;
        Ok(())
    }

    fn render_slice_view(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &RenderTarget<'_>,
        state: &SlicePreviewState,
        bounds: Aabb,
        scalar_range: (f64, f64),
        show_crosshairs: bool,
        colors: ([f32; 4], [f32; 4]),
    ) -> Result<(), RenderEngineError> {
        let (center, width) = state.transfer_window(scalar_range.0, scalar_range.1);
        let window_level = WindowLevel::new(center, width.max(1.0));
        let slice_plane = state.slice_plane(bounds);
        self.renderer.render_slice(
            encoder,
            target.view,
            &slice_plane,
            &window_level,
            target.viewport,
            state.thick_slab().as_ref(),
        )?;

        if show_crosshairs {
            let crosshair_world = state.crosshair_world(bounds);
            let (uv, _) = slice_plane.world_to_point(crosshair_world);
            if (0.0..=1.0).contains(&uv.x) && (0.0..=1.0).contains(&uv.y) {
                self.renderer.render_crosshair(
                    encoder,
                    target.view,
                    target.viewport,
                    &CrosshairParams {
                        position: [uv.x as f32, uv.y as f32],
                        horizontal_color: colors.0,
                        vertical_color: colors.1,
                        thickness: 1.5,
                    },
                )?;
            }
        }

        Ok(())
    }

    fn bounds(&self) -> Result<Aabb, RenderEngineError> {
        let geometry = self.geometry.ok_or(RenderEngineError::NoPreparedVolume)?;
        Ok(bounds_from_geometry(geometry))
    }
}

fn render_params_for_state(
    preset_id: VolumePresetId,
    view_state: VolumeViewState,
    scalar_range: (f64, f64),
) -> VolumeRenderParams {
    let mut params = match view_state.blend_mode {
        VolumeBlendMode::Composite => {
            preset(preset_id, scalar_range.0, scalar_range.1).to_render_params()
        }
        VolumeBlendMode::MaximumIntensity
        | VolumeBlendMode::MinimumIntensity
        | VolumeBlendMode::AverageIntensity => {
            let blend_mode = match view_state.blend_mode {
                VolumeBlendMode::Composite => BlendMode::Composite,
                VolumeBlendMode::MaximumIntensity => BlendMode::MaximumIntensity,
                VolumeBlendMode::MinimumIntensity => BlendMode::MinimumIntensity,
                VolumeBlendMode::AverageIntensity => BlendMode::AverageIntensity,
            };
            VolumeRenderParams::builder()
                .blend_mode(blend_mode)
                .step_size_factor(0.35)
                .color_tf(ColorTransferFunction::greyscale(
                    scalar_range.0,
                    scalar_range.1,
                ))
                .opacity_tf(OpacityTransferFunction::linear_ramp(
                    scalar_range.0,
                    scalar_range.1,
                ))
                .build()
        }
    };
    let (center, width) = view_state.transfer_window(scalar_range.0, scalar_range.1);
    params.window_level = Some(WindowLevel::new(center, width.max(1.0)));
    params
}

fn camera_for_state(geometry: VolumeGeometry, view_state: VolumeViewState) -> Camera {
    let bounds = bounds_from_geometry(geometry);
    let center = bounds.center();
    let diagonal = bounds.diagonal().max(1.0);
    let default_forward = DVec3::Y;
    let default_up = DVec3::NEG_Z;
    let forward = view_state.orientation * default_forward;
    let up = view_state.orientation * default_up;
    let right = forward.cross(up).normalize_or(DVec3::X);
    let fov_y_deg = 30.0_f64;
    let half_diag = diagonal * 0.5;
    let fit_distance = half_diag / (fov_y_deg.to_radians() * 0.5).tan();
    let distance = fit_distance * 1.15 / view_state.zoom.clamp(0.25, 8.0);
    let position = center - forward * distance;
    let pan_scale = distance * 0.001;
    let pan_offset = right * (-view_state.pan_x * pan_scale) + up * (-view_state.pan_y * pan_scale);

    Camera::new(position + pan_offset, center + pan_offset, up)
        .with_projection(Projection::Perspective { fov_y_deg })
        .with_clip_range(
            (distance - diagonal).max(diagonal * 0.01).max(0.1),
            distance + diagonal * 2.0,
        )
}

fn bounds_from_geometry(geometry: VolumeGeometry) -> Aabb {
    let dims = geometry.dimensions.as_dvec3();
    let corners = [
        DVec3::ZERO,
        DVec3::new(dims.x - 1.0, 0.0, 0.0),
        DVec3::new(0.0, dims.y - 1.0, 0.0),
        DVec3::new(0.0, 0.0, dims.z - 1.0),
        DVec3::new(dims.x - 1.0, dims.y - 1.0, 0.0),
        DVec3::new(dims.x - 1.0, 0.0, dims.z - 1.0),
        DVec3::new(0.0, dims.y - 1.0, dims.z - 1.0),
        dims - DVec3::ONE,
    ];
    let world_corners: Vec<DVec3> = corners
        .iter()
        .map(|&corner| geometry.origin + geometry.direction * (corner * geometry.spacing))
        .collect();
    let min = world_corners
        .iter()
        .fold(DVec3::splat(f64::INFINITY), |acc, point| acc.min(*point));
    let max = world_corners
        .iter()
        .fold(DVec3::splat(f64::NEG_INFINITY), |acc, point| {
            acc.max(*point)
        });
    Aabb::new(min, max)
}

fn crosshair_colors(mode: SlicePreviewMode) -> ([f32; 4], [f32; 4]) {
    match mode {
        SlicePreviewMode::Axial => ([0.0, 1.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0]),
        SlicePreviewMode::Coronal => ([0.0, 0.5, 1.0, 1.0], [1.0, 0.0, 0.0, 1.0]),
        SlicePreviewMode::Sagittal => ([0.0, 0.5, 1.0, 1.0], [0.0, 1.0, 0.0, 1.0]),
    }
}

/// Lightweight single-canvas renderer for stack (2D) viewing.
///
/// Unlike [`RenderEngine`] which manages 4 viewports, this engine renders
/// a single slice viewport into one canvas. It uses the same underlying
/// [`VolumeRenderer`] and [`IncrementalVolume`] for data storage and
/// GPU-accelerated reslicing.
pub struct SingleSliceEngine {
    renderer: VolumeRenderer,
    prepared_volume: Option<IncrementalVolume>,
    geometry: Option<VolumeGeometry>,
    slice_state: SlicePreviewState,
    #[allow(dead_code)]
    device: Arc<wgpu::Device>,
    #[allow(dead_code)]
    queue: Arc<wgpu::Queue>,
}

impl SingleSliceEngine {
    /// Creates a single-slice renderer targeting the provided output format.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        output_format: wgpu::TextureFormat,
    ) -> Self {
        Self::from_arc(
            Arc::new(device.clone()),
            Arc::new(queue.clone()),
            output_format,
        )
    }

    /// Creates a single-slice renderer from shared `Arc` handles.
    #[must_use]
    pub fn from_arc(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        output_format: wgpu::TextureFormat,
    ) -> Self {
        Self {
            renderer: VolumeRenderer::from_arc(device.clone(), queue.clone(), output_format),
            prepared_volume: None,
            geometry: None,
            slice_state: SlicePreviewState::default(),
            device,
            queue,
        }
    }

    /// Prepares an empty progressive volume and allocates its GPU texture.
    pub fn prepare_volume(&mut self, geometry: VolumeGeometry) -> Result<(), RenderEngineError> {
        self.prepared_volume = Some(IncrementalVolume::new(geometry)?);
        self.geometry = Some(geometry);
        self.renderer.allocate_volume(
            geometry.dimensions,
            geometry.spacing,
            geometry.origin,
            geometry.direction,
            (0.0, 1.0),
            true,
        );
        Ok(())
    }

    /// Inserts one slice into the progressive volume and uploads it to the GPU.
    pub fn insert_slice(&mut self, z_index: u32, pixels: &[i16]) -> Result<(), RenderEngineError> {
        let volume = self
            .prepared_volume
            .as_mut()
            .ok_or(RenderEngineError::NoPreparedVolume)?;
        volume.insert_slice(z_index, pixels)?;
        let scalar_range = volume
            .scalar_range()
            .map(|(min, max)| (f64::from(min), f64::from(max)))
            .unwrap_or((0.0, 1.0));
        update_texture_slice_i16(&mut self.renderer, z_index, pixels, scalar_range)?;
        Ok(())
    }

    /// Returns the prepared progressive volume, if any.
    #[must_use]
    pub fn prepared_volume(&self) -> Option<&IncrementalVolume> {
        self.prepared_volume.as_ref()
    }

    /// Returns mutable access to the slice viewport state.
    pub fn slice_state_mut(&mut self) -> &mut SlicePreviewState {
        &mut self.slice_state
    }

    /// Returns the currently known scalar range.
    #[must_use]
    pub fn scalar_range(&self) -> Option<(f64, f64)> {
        self.prepared_volume
            .as_ref()
            .and_then(IncrementalVolume::scalar_range)
            .map(|(min, max)| (f64::from(min), f64::from(max)))
    }

    /// Switches which orthogonal plane is displayed.
    pub fn set_slice_mode(&mut self, mode: SlicePreviewMode) {
        self.slice_state.set_mode(mode);
    }

    /// Scrolls the slice along its normal.
    pub fn scroll_slice(&mut self, delta: f64) -> Result<(), RenderEngineError> {
        let bounds = self.bounds()?;
        self.slice_state.scroll_by(delta, bounds);
        Ok(())
    }

    /// Applies a transfer window to the slice viewport.
    pub fn set_window_level(&mut self, center: f64, width: f64) -> Result<(), RenderEngineError> {
        let (scalar_min, scalar_max) = self
            .scalar_range()
            .ok_or(RenderEngineError::NoPreparedVolume)?;
        self.slice_state
            .set_transfer_window(center, width, scalar_min, scalar_max);
        Ok(())
    }

    /// Resets the slice viewport state.
    pub fn reset(&mut self) {
        self.slice_state.reset();
    }

    /// Renders the single slice into the provided target.
    pub fn render_slice(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target: &RenderTarget<'_>,
    ) -> Result<(), RenderEngineError> {
        let _volume = self
            .prepared_volume
            .as_ref()
            .ok_or(RenderEngineError::NoPreparedVolume)?;
        let geometry = self.geometry.ok_or(RenderEngineError::NoPreparedVolume)?;
        let bounds = bounds_from_geometry(geometry);
        let scalar_range = self.scalar_range().unwrap_or((0.0, 1.0));
        let (center, width) = self.slice_state.transfer_window(scalar_range.0, scalar_range.1);
        let window_level = WindowLevel::new(center, width.max(1.0));
        let slice_plane = self.slice_state.slice_plane(bounds);
        self.renderer.render_slice(
            encoder,
            target.view,
            &slice_plane,
            &window_level,
            target.viewport,
            self.slice_state.thick_slab().as_ref(),
        )?;
        Ok(())
    }

    fn bounds(&self) -> Result<Aabb, RenderEngineError> {
        let geometry = self.geometry.ok_or(RenderEngineError::NoPreparedVolume)?;
        Ok(bounds_from_geometry(geometry))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;
    use glam::{DMat3, UVec3};
    use std::sync::mpsc;

    fn geometry() -> VolumeGeometry {
        VolumeGeometry::new(
            UVec3::new(10, 20, 30),
            DVec3::new(0.8, 0.6, 1.2),
            DVec3::ZERO,
            DMat3::IDENTITY,
        )
    }

    #[test]
    fn bounds_match_geometry() {
        let bounds = bounds_from_geometry(geometry());
        assert_abs_diff_eq!(bounds.max.x, 7.2, epsilon = 1e-6);
        assert_abs_diff_eq!(bounds.max.y, 11.4, epsilon = 1e-6);
        assert_abs_diff_eq!(bounds.max.z, 34.8, epsilon = 1e-6);
    }

    #[test]
    fn camera_targets_volume_center() {
        let geometry = geometry();
        let camera = camera_for_state(geometry, VolumeViewState::default());
        let center = bounds_from_geometry(geometry).center();
        assert!((camera.focal_point() - center).length() < 1e-6);
        assert!(camera.distance() > bounds_from_geometry(geometry).diagonal());
    }

    fn test_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                })
                .await
                .ok()?;
            adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .ok()
        })
    }

    fn create_render_texture(device: &wgpu::Device, size: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dicomview_gpu_test_target"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }

    fn read_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let unpadded_bytes_per_row = width * 4;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(256) * 256;
        let buffer_size = u64::from(padded_bytes_per_row) * u64::from(height);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dicomview_gpu_test_readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(std::iter::once(encoder.finish()));

        let (sender, receiver) = mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        receiver.recv().expect("map callback").expect("map success");

        let mapped = buffer.slice(..).get_mapped_range();
        let mut pixels = vec![0u8; (unpadded_bytes_per_row * height) as usize];
        for row in 0..height as usize {
            let src_offset = row * padded_bytes_per_row as usize;
            let dst_offset = row * unpadded_bytes_per_row as usize;
            pixels[dst_offset..dst_offset + unpadded_bytes_per_row as usize]
                .copy_from_slice(&mapped[src_offset..src_offset + unpadded_bytes_per_row as usize]);
        }
        drop(mapped);
        buffer.unmap();
        pixels
    }

    fn checksum(bytes: &[u8]) -> u64 {
        bytes.iter().enumerate().fold(0u64, |acc, (index, value)| {
            acc.wrapping_add((index as u64 + 1) * u64::from(*value))
        })
    }

    #[test]
    #[ignore = "requires a working GPU adapter"]
    fn render_engine_progressive_snapshot_checksum() {
        let Some((device, queue)) = test_device() else {
            return;
        };
        let mut engine = RenderEngine::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        let geometry = VolumeGeometry::new(
            UVec3::new(16, 16, 16),
            DVec3::ONE,
            DVec3::ZERO,
            DMat3::IDENTITY,
        );
        engine.prepare_volume(geometry).expect("prepare volume");

        for z in 0..geometry.dimensions.z {
            let mut slice = vec![0i16; geometry.slice_len()];
            for y in 0..geometry.dimensions.y {
                for x in 0..geometry.dimensions.x {
                    let index = (y * geometry.dimensions.x + x) as usize;
                    let dx = x as f64 - 7.5;
                    let dy = y as f64 - 7.5;
                    let dz = z as f64 - 7.5;
                    if (dx * dx + dy * dy + dz * dz).sqrt() <= 5.0 {
                        slice[index] = 1500;
                    }
                }
            }
            engine.insert_slice(z, &slice).expect("insert slice");
        }

        engine
            .set_crosshair(DVec3::new(8.0, 8.0, 8.0))
            .expect("set crosshair");
        engine.set_thick_slab(
            SlicePreviewMode::Axial,
            6.0,
            SliceProjectionMode::MaximumIntensity,
        );

        let axial_texture = create_render_texture(&device, 96);
        let coronal_texture = create_render_texture(&device, 96);
        let sagittal_texture = create_render_texture(&device, 96);
        let volume_texture = create_render_texture(&device, 96);
        let axial_view = axial_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let coronal_view = coronal_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sagittal_view = sagittal_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let volume_view = volume_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        engine
            .render_frame(
                &mut encoder,
                FrameTargets {
                    axial: RenderTarget {
                        view: &axial_view,
                        viewport: Viewport::full(96, 96),
                    },
                    coronal: RenderTarget {
                        view: &coronal_view,
                        viewport: Viewport::full(96, 96),
                    },
                    sagittal: RenderTarget {
                        view: &sagittal_view,
                        viewport: Viewport::full(96, 96),
                    },
                    volume: RenderTarget {
                        view: &volume_view,
                        viewport: Viewport::full(96, 96),
                    },
                },
                true,
            )
            .expect("render frame");
        queue.submit(std::iter::once(encoder.finish()));

        let axial_pixels = read_texture(&device, &queue, &axial_texture, 96, 96);
        let volume_pixels = read_texture(&device, &queue, &volume_texture, 96, 96);
        assert!(
            checksum(&axial_pixels) > 0,
            "axial slice should not be empty"
        );
        assert!(
            checksum(&volume_pixels) > 0,
            "volume render should not be empty"
        );
    }
}
