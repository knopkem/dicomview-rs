export type ViewportId = "axial" | "coronal" | "sagittal";

export type BlendMode = "composite" | "mip" | "minip" | "average";

export type ProjectionMode = "thin" | "mip" | "minip" | "average";

export type VolumePreset =
  | "ct-bone"
  | "ct-soft-tissue"
  | "ct-lung"
  | "ct-mip"
  | "mr-default"
  | "mr-angio"
  | "mr-t2-brain";

export type WasmSource = string | URL | Request | Response;

export interface VolumeGeometry {
  dimensions: [number, number, number];
  spacing: [number, number, number];
  origin: [number, number, number];
  direction: [
    [number, number, number],
    [number, number, number],
    [number, number, number],
  ];
}

export interface ViewerOptions {
  axial: HTMLCanvasElement;
  coronal: HTMLCanvasElement;
  sagittal: HTMLCanvasElement;
  volume: HTMLCanvasElement;
  wasmUrl?: WasmSource;
}

export interface ThickSlabOptions {
  viewport: ViewportId;
  thickness: number;
  projection: ProjectionMode;
}

export interface SeriesParams {
  studyUid: string;
  seriesUid: string;
}

export type ProgressCallback = (loaded: number, total: number) => void;

export interface DICOMwebLoaderOptions {
  wadoRoot: string;
  fetch?: typeof fetch;
  headers?: HeadersInit;
  concurrency?: number;
  decodeWorkers?: number;
  wasmUrl?: WasmSource;
}
