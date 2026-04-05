//! WASM canvas surface management.

use thiserror::Error;

/// A configured wgpu surface bound to one HTML canvas element.
#[derive(Debug)]
pub struct CanvasSurface {
    /// The underlying wgpu surface.
    pub surface: wgpu::Surface<'static>,
    /// The current surface configuration.
    pub config: wgpu::SurfaceConfiguration,
    /// The presentation format selected for the surface.
    pub format: wgpu::TextureFormat,
    /// Current logical size in pixels.
    pub size: (u32, u32),
}

/// Surface-management errors.
#[derive(Debug, Error)]
pub enum CanvasSurfaceError {
    /// Surface creation failed.
    #[error("failed to create canvas surface: {0}")]
    Create(String),
    /// No supported surface format was available.
    #[error("surface reports no supported formats")]
    NoSupportedFormat,
}

impl CanvasSurface {
    /// Creates and configures a surface for a browser canvas.
    #[cfg(target_arch = "wasm32")]
    pub fn from_canvas(
        instance: &wgpu::Instance,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        canvas: web_sys::HtmlCanvasElement,
        preferred_format: Option<wgpu::TextureFormat>,
    ) -> Result<Self, CanvasSurfaceError> {
        let size = (canvas.width().max(1), canvas.height().max(1));
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|error| CanvasSurfaceError::Create(error.to_string()))?;
        let capabilities = surface.get_capabilities(adapter);
        let format = preferred_format
            .filter(|format| capabilities.formats.contains(format))
            .or_else(|| {
                capabilities.formats.iter().copied().find(|format| {
                    matches!(
                        format,
                        wgpu::TextureFormat::Bgra8Unorm
                            | wgpu::TextureFormat::Bgra8UnormSrgb
                            | wgpu::TextureFormat::Rgba8Unorm
                            | wgpu::TextureFormat::Rgba8UnormSrgb
                    )
                })
            })
            .or_else(|| capabilities.formats.first().copied())
            .ok_or(CanvasSurfaceError::NoSupportedFormat)?;
        let present_mode = capabilities
            .present_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::PresentMode::AutoVsync)
            .unwrap_or(capabilities.present_modes[0]);
        let alpha_mode = capabilities.alpha_modes[0];
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.0,
            height: size.1,
            present_mode,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(device, &config);

        Ok(Self {
            surface,
            config,
            format,
            size,
        })
    }

    /// Resizes and reconfigures the surface.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.size = (width.max(1), height.max(1));
        self.config.width = self.size.0;
        self.config.height = self.size.1;
        self.surface.configure(device, &self.config);
    }
}
