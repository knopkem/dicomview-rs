import initWasm, { Viewer as WasmViewer } from "../wasm/dicomview_wasm.js";
import type {
  BlendMode,
  ProjectionMode,
  ThickSlabOptions,
  ViewerOptions,
  ViewportId,
  VolumeGeometry,
  VolumePreset,
  WasmSource,
} from "./types.js";

const VIEWPORT_CODE: Record<ViewportId, number> = {
  axial: 0,
  coronal: 1,
  sagittal: 2,
};

const BLEND_MODE_CODE: Record<BlendMode, number> = {
  composite: 0,
  mip: 1,
  minip: 2,
  average: 3,
};

const PROJECTION_CODE: Record<ProjectionMode, number> = {
  thin: 0,
  mip: 1,
  minip: 2,
  average: 3,
};

let wasmInitPromise: Promise<void> | null = null;

export async function ensureDicomviewWasm(wasmUrl?: WasmSource): Promise<void> {
  if (!wasmInitPromise) {
    wasmInitPromise = initWasm(wasmUrl);
  }
  await wasmInitPromise;
}

export class Viewer {
  #inner: WasmViewer | null;

  private constructor(inner: WasmViewer) {
    this.#inner = inner;
  }

  static async create(options: ViewerOptions): Promise<Viewer> {
    await ensureDicomviewWasm(options.wasmUrl);
    const inner = await WasmViewer.create({
      axial: options.axial,
      coronal: options.coronal,
      sagittal: options.sagittal,
      volume: options.volume,
    });
    return new Viewer(inner);
  }

  get loadingProgress(): number {
    return this.#requireInner().loading_progress();
  }

  prepareVolume(geometry: VolumeGeometry): void {
    this.#requireInner().prepare_volume(geometry);
  }

  feedDicomSlice(zIndex: number, bytes: ArrayBuffer | ArrayBufferView): void {
    this.#requireInner().feed_dicom_slice(zIndex, toUint8Array(bytes));
  }

  feedPixelSlice(zIndex: number, pixels: Int16Array | ArrayBuffer): void {
    const data = pixels instanceof Int16Array ? pixels : new Int16Array(pixels);
    this.#requireInner().feed_pixel_slice(zIndex, data);
  }

  render(): void {
    this.#requireInner().render();
  }

  setCrosshair(x: number, y: number, z: number): void {
    this.#requireInner().set_crosshair(x, y, z);
  }

  scrollSlice(viewport: ViewportId, delta: number): void {
    this.#requireInner().scroll_slice(VIEWPORT_CODE[viewport], delta);
  }

  setWindowLevel(center: number, width: number): void {
    this.#requireInner().set_window_level(center, width);
  }

  orbit(dx: number, dy: number): void {
    this.#requireInner().orbit(dx, dy);
  }

  pan(dx: number, dy: number): void {
    this.#requireInner().pan(dx, dy);
  }

  zoom(factor: number): void {
    this.#requireInner().zoom(factor);
  }

  setBlendMode(mode: BlendMode): void {
    this.#requireInner().set_blend_mode(BLEND_MODE_CODE[mode]);
  }

  setThickSlab(options: ThickSlabOptions): void {
    this.#requireInner().set_thick_slab(
      VIEWPORT_CODE[options.viewport],
      options.thickness,
      PROJECTION_CODE[options.projection],
    );
  }

  setVolumePreset(preset: VolumePreset): void {
    this.#requireInner().set_volume_preset(preset);
  }

  reset(): void {
    this.#requireInner().reset();
  }

  destroy(): void {
    if (this.#inner) {
      this.#inner.destroy();
      this.#inner = null;
    }
  }

  #requireInner(): WasmViewer {
    if (!this.#inner) {
      throw new Error("Viewer has already been destroyed");
    }
    return this.#inner;
  }
}

function toUint8Array(value: ArrayBuffer | ArrayBufferView): Uint8Array {
  if (value instanceof Uint8Array) {
    return value;
  }
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  }
  return new Uint8Array(value);
}
