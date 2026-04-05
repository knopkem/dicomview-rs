export const Presets = {
  CT_BONE: "ct-bone",
  CT_SOFT_TISSUE: "ct-soft-tissue",
  CT_LUNG: "ct-lung",
  CT_MIP: "ct-mip",
  MR_DEFAULT: "mr-default",
  MR_ANGIO: "mr-angio",
  MR_T2_BRAIN: "mr-t2-brain",
} as const;

/** Cornerstone3D-style preset name aliases. */
export const CornerstonePresets = {
  "CT-Bone": "ct-bone",
  "CT-Soft-Tissue": "ct-soft-tissue",
  "CT-Lung": "ct-lung",
  "CT-MIP": "ct-mip",
  "MR-Default": "mr-default",
  "MR-Angio": "mr-angio",
  "MR-T2-Brain": "mr-t2-brain",
} as const;
