//! Progressive volume construction from arriving slices.

use crate::metadata::VolumeGeometry;
use glam::{DMat3, DVec3, UVec3};
use thiserror::Error;
use volren_core::{DynVolume, Volume, VolumeError, VolumeInfo};

/// Errors raised while creating or updating an [`IncrementalVolume`].
#[derive(Debug, Error, PartialEq)]
pub enum IncrementalVolumeError {
    /// The provided geometry had zero depth or an invalid slice shape.
    #[error("invalid geometry dimensions: {dimensions:?}")]
    InvalidGeometry {
        /// Dimensions that were rejected.
        dimensions: UVec3,
    },
    /// The provided slice length did not match the preallocated geometry.
    #[error("slice {z_index} has {actual} voxels, expected {expected}")]
    SliceLengthMismatch {
        /// Z index that failed validation.
        z_index: u32,
        /// Expected number of voxels in the slice.
        expected: usize,
        /// Actual number of voxels supplied.
        actual: usize,
    },
    /// The requested slice index is outside the preallocated depth.
    #[error("slice index {z_index} is out of bounds for depth {depth}")]
    SliceOutOfBounds {
        /// Invalid slice index.
        z_index: u32,
        /// Preallocated volume depth.
        depth: u32,
    },
    /// The internal volume buffer could not be materialized.
    #[error(transparent)]
    Volume(#[from] VolumeError),
}

/// A volume that can be filled one slice at a time as DICOM frames arrive.
#[derive(Debug, Clone)]
pub struct IncrementalVolume {
    geometry: VolumeGeometry,
    voxels: Vec<i16>,
    loaded_slices: Vec<bool>,
    loaded_count: usize,
    scalar_range: Option<(i16, i16)>,
}

impl IncrementalVolume {
    /// Creates an empty preallocated volume with the provided geometry.
    pub fn new(geometry: VolumeGeometry) -> Result<Self, IncrementalVolumeError> {
        if geometry.dimensions.x == 0 || geometry.dimensions.y == 0 || geometry.dimensions.z == 0 {
            return Err(IncrementalVolumeError::InvalidGeometry {
                dimensions: geometry.dimensions,
            });
        }

        Ok(Self {
            geometry,
            voxels: vec![0; geometry.voxel_count()],
            loaded_slices: vec![false; geometry.dimensions.z as usize],
            loaded_count: 0,
            scalar_range: None,
        })
    }

    /// Inserts or replaces one slice at `z_index`.
    pub fn insert_slice(
        &mut self,
        z_index: u32,
        pixels: &[i16],
    ) -> Result<(), IncrementalVolumeError> {
        if z_index >= self.geometry.dimensions.z {
            return Err(IncrementalVolumeError::SliceOutOfBounds {
                z_index,
                depth: self.geometry.dimensions.z,
            });
        }

        let expected = self.geometry.slice_len();
        if pixels.len() != expected {
            return Err(IncrementalVolumeError::SliceLengthMismatch {
                z_index,
                expected,
                actual: pixels.len(),
            });
        }

        let start = z_index as usize * expected;
        let end = start + expected;
        self.voxels[start..end].copy_from_slice(pixels);

        let was_loaded = std::mem::replace(&mut self.loaded_slices[z_index as usize], true);
        if !was_loaded {
            self.loaded_count += 1;
        }
        self.scalar_range = self.compute_scalar_range();
        Ok(())
    }

    /// Returns the preallocated geometry for the volume.
    #[must_use]
    pub fn geometry(&self) -> VolumeGeometry {
        self.geometry
    }

    /// Returns `true` when every slice has been inserted.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.loaded_count == self.loaded_slices.len()
    }

    /// Returns the number of inserted slices.
    #[must_use]
    pub fn loaded_count(&self) -> usize {
        self.loaded_count
    }

    /// Returns the per-slice loaded mask.
    #[must_use]
    pub fn loaded_mask(&self) -> &[bool] {
        &self.loaded_slices
    }

    /// Returns the loading progress in the range `[0.0, 1.0]`.
    #[must_use]
    pub fn loading_progress(&self) -> f64 {
        self.loaded_count as f64 / self.loaded_slices.len() as f64
    }

    /// Returns the scalar range across the loaded slices.
    #[must_use]
    pub fn scalar_range(&self) -> Option<(i16, i16)> {
        self.scalar_range
    }

    /// Materializes the currently loaded voxels as a typed `Volume<i16>`.
    pub fn as_volume(&self) -> Result<Volume<i16>, IncrementalVolumeError> {
        Ok(Volume::from_data(
            self.voxels.clone(),
            self.geometry.dimensions,
            self.geometry.spacing,
            self.geometry.origin,
            self.geometry.direction,
            1,
        )?)
    }

    /// Materializes the currently loaded voxels as a type-erased `DynVolume`.
    pub fn as_dyn_volume(&self) -> Result<DynVolume, IncrementalVolumeError> {
        Ok(self.as_volume()?.into())
    }

    fn compute_scalar_range(&self) -> Option<(i16, i16)> {
        let slice_len = self.geometry.slice_len();
        let mut min_value = i16::MAX;
        let mut max_value = i16::MIN;
        let mut seen = false;

        for (z_index, loaded) in self.loaded_slices.iter().copied().enumerate() {
            if !loaded {
                continue;
            }
            for &value in &self.voxels[z_index * slice_len..(z_index + 1) * slice_len] {
                min_value = min_value.min(value);
                max_value = max_value.max(value);
                seen = true;
            }
        }

        seen.then_some((min_value, max_value))
    }
}

impl From<Volume<i16>> for VolumeGeometry {
    fn from(volume: Volume<i16>) -> Self {
        Self {
            dimensions: volume.dimensions(),
            spacing: volume.spacing(),
            origin: volume.origin(),
            direction: volume.direction(),
        }
    }
}

impl VolumeGeometry {
    /// Creates geometry from raw volume components.
    #[must_use]
    pub fn new(dimensions: UVec3, spacing: DVec3, origin: DVec3, direction: DMat3) -> Self {
        Self {
            dimensions,
            spacing,
            origin,
            direction,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry() -> VolumeGeometry {
        VolumeGeometry::new(
            UVec3::new(2, 2, 3),
            DVec3::ONE,
            DVec3::ZERO,
            DMat3::IDENTITY,
        )
    }

    #[test]
    fn inserts_slices_in_any_order() {
        let mut volume = IncrementalVolume::new(geometry()).unwrap();
        volume.insert_slice(2, &[9, 10, 11, 12]).unwrap();
        volume.insert_slice(0, &[1, 2, 3, 4]).unwrap();
        volume.insert_slice(1, &[5, 6, 7, 8]).unwrap();

        let typed = volume.as_volume().unwrap();
        assert_eq!(typed.get(0, 0, 0), Some(1));
        assert_eq!(typed.get(1, 1, 1), Some(8));
        assert_eq!(typed.get(0, 0, 2), Some(9));
        assert!(volume.is_complete());
        assert_eq!(volume.scalar_range(), Some((1, 12)));
    }

    #[test]
    fn duplicate_insert_recomputes_scalar_range_without_double_counting() {
        let mut volume = IncrementalVolume::new(geometry()).unwrap();
        volume.insert_slice(0, &[1, 2, 3, 4]).unwrap();
        volume.insert_slice(0, &[10, 20, 30, 40]).unwrap();

        assert_eq!(volume.loaded_count(), 1);
        assert_eq!(volume.scalar_range(), Some((10, 40)));
    }

    #[test]
    fn rejects_bad_slice_length() {
        let err = IncrementalVolume::new(geometry())
            .unwrap()
            .insert_slice(0, &[1, 2, 3])
            .unwrap_err();
        assert!(matches!(
            err,
            IncrementalVolumeError::SliceLengthMismatch {
                expected: 4,
                actual: 3,
                ..
            }
        ));
    }

    #[test]
    fn rejects_out_of_bounds_z() {
        let err = IncrementalVolume::new(geometry())
            .unwrap()
            .insert_slice(3, &[1, 2, 3, 4])
            .unwrap_err();
        assert!(matches!(
            err,
            IncrementalVolumeError::SliceOutOfBounds {
                z_index: 3,
                depth: 3
            }
        ));
    }
}
