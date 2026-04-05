import { DICOMwebLoader, Viewer } from "../../js/dist/index.js";

const WADO_ROOT = "https://your-pacs.example.com/dicom-web";
const STUDY_UID = "1.2.3";
const SERIES_UID = "4.5.6";
const WASM_URL = "../../js/wasm/dicomview_wasm_bg.wasm";

const viewer = await Viewer.create({
  axial: document.getElementById("axial"),
  coronal: document.getElementById("coronal"),
  sagittal: document.getElementById("sagittal"),
  volume: document.getElementById("volume"),
  wasmUrl: WASM_URL,
});

const loader = new DICOMwebLoader({
  wadoRoot: WADO_ROOT,
  decodeWorkers: 2,
  wasmUrl: WASM_URL,
});

await loader.loadSeries(viewer, {
  studyUid: STUDY_UID,
  seriesUid: SERIES_UID,
});

viewer.setVolumePreset(document.getElementById("preset").value);
viewer.render();

document.getElementById("preset").addEventListener("change", (event) => {
  viewer.setVolumePreset(event.target.value);
  viewer.render();
});

document.getElementById("mip").addEventListener("click", () => {
  viewer.setBlendMode("mip");
  viewer.render();
});
