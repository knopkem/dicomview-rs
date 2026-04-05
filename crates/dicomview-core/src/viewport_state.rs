//! Pure data and math for MPR and volume viewport state.

use glam::{DQuat, DVec3};
use volren_core::{Aabb, SlicePlane, ThickSlabMode, ThickSlabParams};

/// Blend modes exposed by the dicomview volume viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VolumeBlendMode {
    /// Front-to-back compositing.
    #[default]
    Composite,
    /// Maximum intensity projection.
    MaximumIntensity,
    /// Minimum intensity projection.
    MinimumIntensity,
    /// Average intensity projection.
    AverageIntensity,
}

/// One of the three orthogonal MPR orientations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlicePreviewMode {
    /// Axial plane.
    #[default]
    Axial,
    /// Coronal plane.
    Coronal,
    /// Sagittal plane.
    Sagittal,
}

/// Projection style used when rendering one reslice viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SliceProjectionMode {
    /// Thin single-voxel slice.
    #[default]
    Thin,
    /// Maximum intensity slab projection.
    MaximumIntensity,
    /// Minimum intensity slab projection.
    MinimumIntensity,
    /// Mean intensity slab projection.
    AverageIntensity,
}

/// Mutable state for one MPR viewport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlicePreviewState {
    /// Active orthogonal slice family.
    pub mode: SlicePreviewMode,
    /// Signed offset from the volume center along the current slice normal.
    pub offset: f64,
    /// Additional quaternion rotation applied around the shared MPR cursor.
    pub orientation: DQuat,
    /// Thin-slice vs slab projection mode.
    pub projection_mode: SliceProjectionMode,
    /// Half-thickness of the active slab in world units.
    pub slab_half_thickness: f64,
    /// Shared MPR cursor in world space.
    pub crosshair_world: Option<DVec3>,
    /// Explicit transfer-window center in modality space.
    pub transfer_center_hu: Option<f64>,
    /// Explicit transfer-window width in modality space.
    pub transfer_width_hu: Option<f64>,
    slab_settings_by_mode: [SliceSlabSettings; 3],
}

impl Default for SlicePreviewState {
    fn default() -> Self {
        let slab_settings = [SliceSlabSettings::default(); 3];
        Self {
            mode: SlicePreviewMode::Axial,
            offset: 0.0,
            orientation: DQuat::IDENTITY,
            projection_mode: slab_settings[0].projection_mode,
            slab_half_thickness: slab_settings[0].slab_half_thickness,
            crosshair_world: None,
            transfer_center_hu: None,
            transfer_width_hu: None,
            slab_settings_by_mode: slab_settings,
        }
    }
}

impl SlicePreviewState {
    /// Ensures that the state has a transfer window appropriate for the scalar range.
    pub fn ensure_transfer_window(&mut self, scalar_min: f64, scalar_max: f64) {
        let (center, width) = resolved_slice_transfer_window(*self, scalar_min, scalar_max);
        self.transfer_center_hu.get_or_insert(center);
        self.transfer_width_hu.get_or_insert(width);
    }

    /// Returns the current slice transfer window, falling back to a safe default.
    #[must_use]
    pub fn transfer_window(&self, scalar_min: f64, scalar_max: f64) -> (f64, f64) {
        resolved_slice_transfer_window(*self, scalar_min, scalar_max)
    }

    /// Updates the slice transfer window with clamping.
    pub fn set_transfer_window(
        &mut self,
        center: f64,
        width: f64,
        scalar_min: f64,
        scalar_max: f64,
    ) {
        let (center, width) = clamp_transfer_window(center, width, scalar_min, scalar_max);
        self.transfer_center_hu = Some(center);
        self.transfer_width_hu = Some(width);
    }

    /// Resets the slice state back to the default centered slice.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Switches the viewport to another orthogonal slice family.
    pub fn set_mode(&mut self, mode: SlicePreviewMode) {
        self.persist_current_slab_settings();
        self.mode = mode;
        self.restore_current_slab_settings();
    }

    /// Resolves the current oriented slice plane within the provided bounds.
    #[must_use]
    pub fn slice_plane(&self, bounds: Aabb) -> SlicePlane {
        slice_plane_for_state(bounds, *self)
    }

    /// Returns the active shared crosshair, defaulting to the volume center.
    #[must_use]
    pub fn crosshair_world(&self, bounds: Aabb) -> DVec3 {
        self.crosshair_world.unwrap_or(bounds.center())
    }

    /// Updates the shared crosshair point.
    pub fn set_crosshair_world(&mut self, world: DVec3) {
        self.crosshair_world = Some(world);
    }

    /// Moves the slice so it passes through `world`.
    pub fn center_on_world(&mut self, world: DVec3, bounds: Aabb) {
        let center = bounds.center();
        let normal = self.slice_plane(bounds).normal();
        let unclamped_offset = (world - center).dot(normal);
        self.offset = unclamped_offset;
        self.clamp_offset(bounds);
        self.crosshair_world = Some(world + normal * (self.offset - unclamped_offset));
    }

    /// Moves the slice so it passes through the shared crosshair.
    pub fn center_on_crosshair(&mut self, bounds: Aabb) {
        self.center_on_world(self.crosshair_world(bounds), bounds);
    }

    /// Cycles between thin-slice and the supported slab modes.
    pub fn cycle_projection_mode(&mut self, default_half_thickness: f64) {
        self.projection_mode = match self.projection_mode {
            SliceProjectionMode::Thin => SliceProjectionMode::MaximumIntensity,
            SliceProjectionMode::MaximumIntensity => SliceProjectionMode::MinimumIntensity,
            SliceProjectionMode::MinimumIntensity => SliceProjectionMode::AverageIntensity,
            SliceProjectionMode::AverageIntensity => SliceProjectionMode::Thin,
        };
        self.slab_half_thickness = if matches!(self.projection_mode, SliceProjectionMode::Thin) {
            0.0
        } else {
            default_half_thickness.max(0.5)
        };
        self.persist_current_slab_settings();
    }

    /// Adjusts slab thickness based on pointer drag semantics.
    pub fn set_slab_half_thickness_from_drag(
        &mut self,
        half_thickness: f64,
        min_active_half_thickness: f64,
        fallback_mode: SliceProjectionMode,
    ) {
        if half_thickness <= min_active_half_thickness {
            self.projection_mode = SliceProjectionMode::Thin;
            self.slab_half_thickness = 0.0;
        } else {
            if matches!(self.projection_mode, SliceProjectionMode::Thin) {
                self.projection_mode = fallback_mode;
            }
            self.slab_half_thickness = half_thickness.max(0.5);
        }
        self.persist_current_slab_settings();
    }

    /// Resolves the thick-slab parameters for the current projection state.
    #[must_use]
    pub fn thick_slab(self) -> Option<ThickSlabParams> {
        let mode = match self.projection_mode {
            SliceProjectionMode::Thin => return None,
            SliceProjectionMode::MaximumIntensity => ThickSlabMode::Mip,
            SliceProjectionMode::MinimumIntensity => ThickSlabMode::MinIp,
            SliceProjectionMode::AverageIntensity => ThickSlabMode::Mean,
        };
        Some(ThickSlabParams {
            half_thickness: self.slab_half_thickness.max(0.5),
            mode,
            num_samples: 16,
        })
    }

    /// Clamps the current slice offset to the volume bounds.
    pub fn clamp_offset(&mut self, bounds: Aabb) {
        let (min_offset, max_offset) =
            slice_offset_range(bounds, self.slice_plane(bounds).normal());
        self.offset = self.offset.clamp(min_offset, max_offset);
    }

    /// Scrolls along the current slice normal.
    pub fn scroll_by(&mut self, delta: f64, bounds: Aabb) {
        let world = self.crosshair_world(bounds) + self.slice_plane(bounds).normal() * delta;
        self.center_on_world(world, bounds);
    }

    /// Rotates the slice around its current normal axis.
    pub fn rotate_about_normal(&mut self, angle_rad: f64, bounds: Aabb) {
        let axis = self.slice_plane(bounds).normal();
        let rotation = DQuat::from_axis_angle(axis.normalize_or(DVec3::Z), angle_rad);
        self.orientation = (rotation * self.orientation).normalize();
        self.center_on_crosshair(bounds);
    }

    fn persist_current_slab_settings(&mut self) {
        self.slab_settings_by_mode[mode_index(self.mode)] = SliceSlabSettings {
            projection_mode: self.projection_mode,
            slab_half_thickness: self.slab_half_thickness,
        };
    }

    fn restore_current_slab_settings(&mut self) {
        let settings = self.slab_settings_by_mode[mode_index(self.mode)];
        self.projection_mode = settings.projection_mode;
        self.slab_half_thickness = settings.slab_half_thickness;
    }
}

/// Mutable camera and transfer state for the 3D volume viewport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumeViewState {
    /// Accumulated camera orientation relative to the default AP view.
    pub orientation: DQuat,
    /// Horizontal pan in screen-like units.
    pub pan_x: f64,
    /// Vertical pan in screen-like units.
    pub pan_y: f64,
    /// Camera zoom factor.
    pub zoom: f64,
    /// Active raycasting blend mode.
    pub blend_mode: VolumeBlendMode,
    /// Explicit transfer-window center in modality space.
    pub transfer_center_hu: Option<f64>,
    /// Explicit transfer-window width in modality space.
    pub transfer_width_hu: Option<f64>,
}

impl Default for VolumeViewState {
    fn default() -> Self {
        Self {
            orientation: DQuat::IDENTITY,
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
            blend_mode: VolumeBlendMode::Composite,
            transfer_center_hu: None,
            transfer_width_hu: None,
        }
    }
}

impl VolumeViewState {
    /// Orbits the virtual camera around the volume center.
    pub fn orbit(&mut self, delta_x: f64, delta_y: f64) {
        let yaw = DQuat::from_axis_angle(DVec3::Z, -delta_x.to_radians());
        let local_right = self.orientation * DVec3::X;
        let pitch = DQuat::from_axis_angle(local_right, -delta_y.to_radians());
        self.orientation = (pitch * yaw * self.orientation).normalize();
    }

    /// Pans the camera in the view plane.
    pub fn pan(&mut self, delta_x: f64, delta_y: f64) {
        self.pan_x += delta_x;
        self.pan_y += delta_y;
    }

    /// Applies a multiplicative zoom factor.
    pub fn zoom_by(&mut self, factor: f64) {
        self.zoom = (self.zoom * factor).clamp(0.25, 8.0);
    }

    /// Ensures that the state has a reasonable transfer window for the scalar range.
    pub fn ensure_transfer_window(&mut self, scalar_min: f64, scalar_max: f64) {
        let (center, width) = resolved_transfer_window(*self, scalar_min, scalar_max);
        self.transfer_center_hu.get_or_insert(center);
        self.transfer_width_hu.get_or_insert(width);
    }

    /// Returns the active transfer window or a derived default.
    #[must_use]
    pub fn transfer_window(&self, scalar_min: f64, scalar_max: f64) -> (f64, f64) {
        resolved_transfer_window(*self, scalar_min, scalar_max)
    }

    /// Updates the transfer window with clamping.
    pub fn set_transfer_window(
        &mut self,
        center: f64,
        width: f64,
        scalar_min: f64,
        scalar_max: f64,
    ) {
        let (center, width) = clamp_transfer_window(center, width, scalar_min, scalar_max);
        self.transfer_center_hu = Some(center);
        self.transfer_width_hu = Some(width);
    }

    /// Resets the volume viewport state back to defaults.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SliceSlabSettings {
    projection_mode: SliceProjectionMode,
    slab_half_thickness: f64,
}

impl Default for SliceSlabSettings {
    fn default() -> Self {
        Self {
            projection_mode: SliceProjectionMode::Thin,
            slab_half_thickness: 0.0,
        }
    }
}

fn mode_index(mode: SlicePreviewMode) -> usize {
    match mode {
        SlicePreviewMode::Axial => 0,
        SlicePreviewMode::Coronal => 1,
        SlicePreviewMode::Sagittal => 2,
    }
}

fn looks_ct_like(scalar_min: f64, scalar_max: f64) -> bool {
    scalar_min <= -500.0 && scalar_max >= 1200.0
}

fn resolved_transfer_window(
    view_state: VolumeViewState,
    scalar_min: f64,
    scalar_max: f64,
) -> (f64, f64) {
    let range = (scalar_max - scalar_min).max(1.0);
    let default_center = if looks_ct_like(scalar_min, scalar_max) {
        90.0
    } else {
        scalar_min + range * 0.5
    };
    let default_width = if looks_ct_like(scalar_min, scalar_max) {
        700.0
    } else {
        range
    };
    clamp_transfer_window(
        view_state.transfer_center_hu.unwrap_or(default_center),
        view_state.transfer_width_hu.unwrap_or(default_width),
        scalar_min,
        scalar_max,
    )
}

fn resolved_slice_transfer_window(
    view_state: SlicePreviewState,
    scalar_min: f64,
    scalar_max: f64,
) -> (f64, f64) {
    let range = (scalar_max - scalar_min).max(1.0);
    clamp_transfer_window(
        view_state
            .transfer_center_hu
            .unwrap_or(scalar_min + range * 0.5),
        view_state.transfer_width_hu.unwrap_or(range),
        scalar_min,
        scalar_max,
    )
}

fn clamp_transfer_window(center: f64, width: f64, scalar_min: f64, scalar_max: f64) -> (f64, f64) {
    let range = (scalar_max - scalar_min).max(1.0);
    (
        center.clamp(scalar_min - range * 0.25, scalar_max + range * 0.25),
        width.clamp(range / 200.0, range * 1.25),
    )
}

fn slice_basis_for_mode(mode: SlicePreviewMode) -> (DVec3, DVec3) {
    match mode {
        SlicePreviewMode::Axial => (DVec3::X, DVec3::Y),
        SlicePreviewMode::Coronal => (DVec3::X, -DVec3::Z),
        SlicePreviewMode::Sagittal => (DVec3::Y, -DVec3::Z),
    }
}

fn slice_preferred_up_for_mode(mode: SlicePreviewMode) -> DVec3 {
    match mode {
        SlicePreviewMode::Axial => DVec3::Y,
        SlicePreviewMode::Coronal | SlicePreviewMode::Sagittal => -DVec3::Z,
    }
}

fn slice_basis_from_normal(mode: SlicePreviewMode, normal: DVec3) -> (DVec3, DVec3) {
    let project_reference = |reference: DVec3| {
        let projected = reference - normal * reference.dot(normal);
        (projected.length_squared() > 1.0e-10).then(|| projected.normalize())
    };

    let up = project_reference(slice_preferred_up_for_mode(mode))
        .or_else(|| {
            [DVec3::X, DVec3::Y, DVec3::Z]
                .into_iter()
                .find_map(project_reference)
        })
        .unwrap_or(DVec3::Y);
    let right = up.cross(normal).normalize_or(DVec3::X);
    let up = normal.cross(right).normalize_or(up);
    (right, up)
}

fn slice_offset_range(bounds: Aabb, normal: DVec3) -> (f64, f64) {
    let center = bounds.center();
    let corners = [
        DVec3::new(bounds.min.x, bounds.min.y, bounds.min.z),
        DVec3::new(bounds.min.x, bounds.min.y, bounds.max.z),
        DVec3::new(bounds.min.x, bounds.max.y, bounds.min.z),
        DVec3::new(bounds.min.x, bounds.max.y, bounds.max.z),
        DVec3::new(bounds.max.x, bounds.min.y, bounds.min.z),
        DVec3::new(bounds.max.x, bounds.min.y, bounds.max.z),
        DVec3::new(bounds.max.x, bounds.max.y, bounds.min.z),
        DVec3::new(bounds.max.x, bounds.max.y, bounds.max.z),
    ];

    let mut min_offset = f64::INFINITY;
    let mut max_offset = f64::NEG_INFINITY;
    for corner in corners {
        let offset = (corner - center).dot(normal);
        min_offset = min_offset.min(offset);
        max_offset = max_offset.max(offset);
    }
    (min_offset, max_offset)
}

fn slice_plane_for_state(bounds: Aabb, view_state: SlicePreviewState) -> SlicePlane {
    let center = bounds.center();
    let size = bounds.size();
    let (base_right, base_up) = slice_basis_for_mode(view_state.mode);
    let default_normal = base_right.cross(base_up).normalize_or(DVec3::Z);
    let normal = (view_state.orientation * default_normal).normalize_or(default_normal);
    let (right, up) = slice_basis_from_normal(view_state.mode, normal);
    let (min_offset, max_offset) = slice_offset_range(bounds, normal);
    let clamped_offset = view_state.offset.clamp(min_offset, max_offset);
    let origin = center + normal * clamped_offset;

    match view_state.mode {
        SlicePreviewMode::Axial => {
            SlicePlane::new(origin, right, up, size.x.max(1.0), size.y.max(1.0))
        }
        SlicePreviewMode::Coronal => {
            SlicePlane::new(origin, right, up, size.x.max(1.0), size.z.max(1.0))
        }
        SlicePreviewMode::Sagittal => {
            SlicePlane::new(origin, right, up, size.y.max(1.0), size.z.max(1.0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_view_state_orbit_and_zoom_clamp() {
        let mut state = VolumeViewState::default();
        state.orbit(10.0, 200.0);
        state.zoom_by(100.0);
        assert_ne!(state.orientation, DQuat::IDENTITY);
        assert_eq!(state.zoom, 8.0);
    }

    #[test]
    fn transfer_window_defaults_to_soft_tissue_for_ct() {
        let mut state = VolumeViewState::default();
        state.ensure_transfer_window(-1024.0, 3071.0);
        assert_eq!(state.transfer_window(-1024.0, 3071.0), (90.0, 700.0));
    }

    #[test]
    fn slice_preview_state_clamps_scroll_to_volume_bounds() {
        let bounds = Aabb::new(DVec3::ZERO, DVec3::new(10.0, 20.0, 30.0));
        let mut state = SlicePreviewState::default();
        state.scroll_by(100.0, bounds);
        assert_eq!(state.offset, 15.0);
        state.scroll_by(-100.0, bounds);
        assert_eq!(state.offset, -15.0);
    }

    #[test]
    fn slice_projection_mode_is_remembered_per_axis() {
        let mut state = SlicePreviewState::default();
        state.cycle_projection_mode(6.0);
        assert_eq!(state.projection_mode, SliceProjectionMode::MaximumIntensity);
        assert_eq!(state.slab_half_thickness, 6.0);

        state.set_mode(SlicePreviewMode::Coronal);
        assert_eq!(state.projection_mode, SliceProjectionMode::Thin);
        state.cycle_projection_mode(10.0);
        state.cycle_projection_mode(10.0);
        assert_eq!(state.projection_mode, SliceProjectionMode::MinimumIntensity);
        assert_eq!(state.slab_half_thickness, 10.0);

        state.set_mode(SlicePreviewMode::Axial);
        assert_eq!(state.projection_mode, SliceProjectionMode::MaximumIntensity);
        assert_eq!(state.slab_half_thickness, 6.0);
    }

    #[test]
    fn slice_default_planes_follow_radiology_view_conventions() {
        let bounds = Aabb::new(DVec3::ZERO, DVec3::new(10.0, 20.0, 30.0));

        let mut coronal = SlicePreviewState::default();
        coronal.set_mode(SlicePreviewMode::Coronal);
        let coronal_plane = coronal.slice_plane(bounds);
        assert!(coronal_plane.right.distance(DVec3::X) < 1.0e-6);
        assert!(coronal_plane.up.distance(-DVec3::Z) < 1.0e-6);

        let mut sagittal = SlicePreviewState::default();
        sagittal.set_mode(SlicePreviewMode::Sagittal);
        let sagittal_plane = sagittal.slice_plane(bounds);
        assert!(sagittal_plane.right.distance(DVec3::Y) < 1.0e-6);
        assert!(sagittal_plane.up.distance(-DVec3::Z) < 1.0e-6);
    }
}
