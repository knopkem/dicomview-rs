//! Volume-rendering presets for CT and MR style datasets.

use volren_core::{
    BlendMode, ColorSpace, ColorTransferFunction, OpacityTransferFunction, ShadingParams,
    TransferFunctionLut, VolumeRenderParams, WindowLevel,
};

/// Identifier for one built-in transfer-function preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumePresetId {
    /// CT bone rendering preset.
    CtBone,
    /// CT soft-tissue rendering preset.
    CtSoftTissue,
    /// CT lung rendering preset.
    CtLung,
    /// CT MIP preset.
    CtMip,
    /// Generic MR composite preset.
    MrDefault,
    /// MR angiography MIP preset.
    MrAngio,
    /// T2-weighted MR brain preset.
    MrT2Brain,
}

/// One fully materialized preset with transfer functions and render settings.
#[derive(Debug, Clone)]
pub struct VolumePreset {
    /// Identifier of the preset.
    pub id: VolumePresetId,
    /// Human-readable label.
    pub label: &'static str,
    /// Raycasting blend mode.
    pub blend_mode: BlendMode,
    /// Color transfer function.
    pub color_tf: ColorTransferFunction,
    /// Opacity transfer function.
    pub opacity_tf: OpacityTransferFunction,
    /// Optional shading parameters.
    pub shading: Option<ShadingParams>,
    /// Optional window-level metadata associated with the preset.
    pub window_level: Option<WindowLevel>,
    /// Ray step-size factor to use for rendering.
    pub step_size_factor: f32,
}

impl VolumePreset {
    /// Bakes the preset transfer functions into a 1D LUT.
    #[must_use]
    pub fn bake_lut(&self, scalar_min: f64, scalar_max: f64, lut_size: u32) -> TransferFunctionLut {
        TransferFunctionLut::bake(
            &self.color_tf,
            &self.opacity_tf,
            scalar_min,
            scalar_max,
            lut_size,
        )
    }

    /// Converts the preset into `volren-core` render parameters.
    #[must_use]
    pub fn to_render_params(&self) -> VolumeRenderParams {
        let mut builder = VolumeRenderParams::builder()
            .blend_mode(self.blend_mode)
            .color_tf(self.color_tf.clone())
            .opacity_tf(self.opacity_tf.clone())
            .step_size_factor(self.step_size_factor);
        if let Some(window_level) = self.window_level {
            builder = builder.window_level(window_level);
        }
        if let Some(shading) = self.shading {
            builder = builder.shading(shading);
        } else {
            builder = builder.no_shading();
        }
        builder.build()
    }
}

/// Returns all built-in preset identifiers in stable order.
#[must_use]
pub fn preset_ids() -> &'static [VolumePresetId] {
    &[
        VolumePresetId::CtBone,
        VolumePresetId::CtSoftTissue,
        VolumePresetId::CtLung,
        VolumePresetId::CtMip,
        VolumePresetId::MrDefault,
        VolumePresetId::MrAngio,
        VolumePresetId::MrT2Brain,
    ]
}

/// Materializes one preset for the provided scalar range.
#[must_use]
pub fn preset(id: VolumePresetId, scalar_min: f64, scalar_max: f64) -> VolumePreset {
    match id {
        VolumePresetId::CtBone => ct_bone_preset(scalar_min, scalar_max),
        VolumePresetId::CtSoftTissue => ct_soft_tissue_preset(scalar_min, scalar_max),
        VolumePresetId::CtLung => ct_lung_preset(scalar_min, scalar_max),
        VolumePresetId::CtMip => ct_mip_preset(scalar_min, scalar_max),
        VolumePresetId::MrDefault => mr_default_preset(scalar_min, scalar_max),
        VolumePresetId::MrAngio => mr_angio_preset(scalar_min, scalar_max),
        VolumePresetId::MrT2Brain => mr_t2_brain_preset(scalar_min, scalar_max),
    }
}

fn ct_bone_preset(scalar_min: f64, scalar_max: f64) -> VolumePreset {
    let mut color_tf = ColorTransferFunction::new(ColorSpace::Rgb);
    color_tf.add_point(scalar_min, [0.0, 0.0, 0.0]);
    color_tf.add_point(-200.0, [0.05, 0.05, 0.05]);
    color_tf.add_point(300.0, [0.82, 0.72, 0.62]);
    color_tf.add_point(1200.0, [1.0, 0.98, 0.95]);
    color_tf.add_point(scalar_max, [1.0, 1.0, 1.0]);

    let mut opacity_tf = OpacityTransferFunction::new();
    opacity_tf.add_point(scalar_min, 0.0);
    opacity_tf.add_point(150.0, 0.0);
    opacity_tf.add_point(300.0, 0.20);
    opacity_tf.add_point(700.0, 0.65);
    opacity_tf.add_point(scalar_max, 0.95);

    VolumePreset {
        id: VolumePresetId::CtBone,
        label: "CT Bone",
        blend_mode: BlendMode::Composite,
        color_tf,
        opacity_tf,
        shading: Some(ShadingParams {
            ambient: 0.15,
            diffuse: 0.50,
            specular: 1.05,
            specular_power: 54.0,
        }),
        window_level: Some(WindowLevel::new(500.0, 2000.0)),
        step_size_factor: 0.45,
    }
}

fn ct_soft_tissue_preset(scalar_min: f64, scalar_max: f64) -> VolumePreset {
    let mut color_tf = ColorTransferFunction::new(ColorSpace::Rgb);
    color_tf.add_point(scalar_min, [0.0, 0.0, 0.0]);
    color_tf.add_point(-150.0, [0.18, 0.12, 0.10]);
    color_tf.add_point(40.0, [0.72, 0.40, 0.36]);
    color_tf.add_point(250.0, [0.92, 0.76, 0.70]);
    color_tf.add_point(scalar_max, [1.0, 1.0, 1.0]);

    let mut opacity_tf = OpacityTransferFunction::new();
    opacity_tf.add_point(scalar_min, 0.0);
    opacity_tf.add_point(-120.0, 0.0);
    opacity_tf.add_point(-20.0, 0.04);
    opacity_tf.add_point(80.0, 0.22);
    opacity_tf.add_point(220.0, 0.18);
    opacity_tf.add_point(500.0, 0.02);
    opacity_tf.add_point(scalar_max, 0.0);

    VolumePreset {
        id: VolumePresetId::CtSoftTissue,
        label: "CT Soft Tissue",
        blend_mode: BlendMode::Composite,
        color_tf,
        opacity_tf,
        shading: Some(ShadingParams {
            ambient: 0.45,
            diffuse: 0.70,
            specular: 0.60,
            specular_power: 17.0,
        }),
        window_level: Some(WindowLevel::new(40.0, 400.0)),
        step_size_factor: 0.55,
    }
}

fn ct_lung_preset(scalar_min: f64, scalar_max: f64) -> VolumePreset {
    let mut color_tf = ColorTransferFunction::new(ColorSpace::Rgb);
    color_tf.add_point(scalar_min, [0.0, 0.0, 0.0]);
    color_tf.add_point(-1000.0, [0.0, 0.0, 0.0]);
    color_tf.add_point(-800.0, [0.20, 0.35, 0.65]);
    color_tf.add_point(-500.0, [0.80, 0.82, 0.92]);
    color_tf.add_point(200.0, [1.0, 0.96, 0.92]);
    color_tf.add_point(scalar_max, [1.0, 1.0, 1.0]);

    let mut opacity_tf = OpacityTransferFunction::new();
    opacity_tf.add_point(scalar_min, 0.0);
    opacity_tf.add_point(-950.0, 0.0);
    opacity_tf.add_point(-800.0, 0.05);
    opacity_tf.add_point(-650.0, 0.20);
    opacity_tf.add_point(-300.0, 0.08);
    opacity_tf.add_point(200.0, 0.02);
    opacity_tf.add_point(scalar_max, 0.0);

    VolumePreset {
        id: VolumePresetId::CtLung,
        label: "CT Lung",
        blend_mode: BlendMode::Composite,
        color_tf,
        opacity_tf,
        shading: Some(ShadingParams::default()),
        window_level: Some(WindowLevel::new(-600.0, 1500.0)),
        step_size_factor: 0.5,
    }
}

fn ct_mip_preset(scalar_min: f64, scalar_max: f64) -> VolumePreset {
    VolumePreset {
        id: VolumePresetId::CtMip,
        label: "CT MIP",
        blend_mode: BlendMode::MaximumIntensity,
        color_tf: ColorTransferFunction::greyscale(scalar_min, scalar_max),
        opacity_tf: OpacityTransferFunction::linear_ramp(scalar_min, scalar_max),
        shading: None,
        window_level: Some(WindowLevel::new(400.0, 1600.0)),
        step_size_factor: 0.35,
    }
}

fn mr_default_preset(scalar_min: f64, scalar_max: f64) -> VolumePreset {
    let mut color_tf = ColorTransferFunction::new(ColorSpace::Rgb);
    color_tf.add_point(scalar_min, [0.0, 0.0, 0.0]);
    color_tf.add_point(
        scalar_min + (scalar_max - scalar_min) * 0.25,
        [0.15, 0.15, 0.22],
    );
    color_tf.add_point(
        scalar_min + (scalar_max - scalar_min) * 0.65,
        [0.75, 0.75, 0.82],
    );
    color_tf.add_point(scalar_max, [1.0, 1.0, 1.0]);

    let mut opacity_tf = OpacityTransferFunction::new();
    opacity_tf.add_point(scalar_min, 0.0);
    opacity_tf.add_point(scalar_min + (scalar_max - scalar_min) * 0.15, 0.0);
    opacity_tf.add_point(scalar_min + (scalar_max - scalar_min) * 0.45, 0.12);
    opacity_tf.add_point(scalar_max, 0.88);

    VolumePreset {
        id: VolumePresetId::MrDefault,
        label: "MR Default",
        blend_mode: BlendMode::Composite,
        color_tf,
        opacity_tf,
        shading: Some(ShadingParams::default()),
        window_level: None,
        step_size_factor: 0.55,
    }
}

fn mr_angio_preset(scalar_min: f64, scalar_max: f64) -> VolumePreset {
    let mut color_tf = ColorTransferFunction::new(ColorSpace::Rgb);
    color_tf.add_point(scalar_min, [0.0, 0.0, 0.0]);
    color_tf.add_point(
        scalar_min + (scalar_max - scalar_min) * 0.55,
        [0.35, 0.35, 0.38],
    );
    color_tf.add_point(scalar_max, [1.0, 1.0, 1.0]);

    VolumePreset {
        id: VolumePresetId::MrAngio,
        label: "MR Angio",
        blend_mode: BlendMode::MaximumIntensity,
        color_tf,
        opacity_tf: OpacityTransferFunction::linear_ramp(scalar_min, scalar_max),
        shading: None,
        window_level: None,
        step_size_factor: 0.35,
    }
}

fn mr_t2_brain_preset(scalar_min: f64, scalar_max: f64) -> VolumePreset {
    let mut color_tf = ColorTransferFunction::new(ColorSpace::Rgb);
    color_tf.add_point(scalar_min, [0.0, 0.0, 0.0]);
    color_tf.add_point(
        scalar_min + (scalar_max - scalar_min) * 0.30,
        [0.16, 0.18, 0.30],
    );
    color_tf.add_point(
        scalar_min + (scalar_max - scalar_min) * 0.65,
        [0.72, 0.78, 0.94],
    );
    color_tf.add_point(scalar_max, [1.0, 1.0, 1.0]);

    let mut opacity_tf = OpacityTransferFunction::new();
    opacity_tf.add_point(scalar_min, 0.0);
    opacity_tf.add_point(scalar_min + (scalar_max - scalar_min) * 0.10, 0.0);
    opacity_tf.add_point(scalar_min + (scalar_max - scalar_min) * 0.45, 0.08);
    opacity_tf.add_point(scalar_min + (scalar_max - scalar_min) * 0.70, 0.35);
    opacity_tf.add_point(scalar_max, 0.90);

    VolumePreset {
        id: VolumePresetId::MrT2Brain,
        label: "MR T2 Brain",
        blend_mode: BlendMode::Composite,
        color_tf,
        opacity_tf,
        shading: Some(ShadingParams::default()),
        window_level: None,
        step_size_factor: 0.5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_id_list_is_stable() {
        assert_eq!(preset_ids().len(), 7);
        assert_eq!(preset_ids()[0], VolumePresetId::CtBone);
        assert_eq!(preset_ids()[6], VolumePresetId::MrT2Brain);
    }

    #[test]
    fn ct_bone_lut_has_high_opacity_near_upper_end() {
        let preset = preset(VolumePresetId::CtBone, -1024.0, 3071.0);
        let lut = preset.bake_lut(-1024.0, 3071.0, 256);
        let rgba = lut.as_rgba_f32();
        let alpha_near_end = rgba[(220 * 4) + 3];
        assert!(alpha_near_end > 0.5);
    }

    #[test]
    fn ct_mip_uses_mip_blend_mode() {
        let preset = preset(VolumePresetId::CtMip, -1024.0, 3071.0);
        assert!(matches!(preset.blend_mode, BlendMode::MaximumIntensity));
        assert!(preset.shading.is_none());
    }

    #[test]
    fn mr_default_lut_is_non_empty() {
        let preset = preset(VolumePresetId::MrDefault, 0.0, 4095.0);
        let lut = preset.bake_lut(0.0, 4095.0, 128);
        assert_eq!(lut.as_rgba_f32().len(), 128 * 4);
        assert!(preset.color_tf.len() >= 3);
        assert!(preset.opacity_tf.len() >= 3);
    }
}
