import type { WasmSource } from "../src/types.js";

export default function init(moduleOrPath?: WasmSource): Promise<void>;

export class Viewer {
  static create(config: unknown): Promise<Viewer>;
  prepare_volume(geometry: unknown): void;
  feed_dicom_slice(zIndex: number, bytes: Uint8Array): void;
  feed_pixel_slice(zIndex: number, pixels: Int16Array): void;
  loading_progress(): number;
  render(): void;
  set_crosshair(x: number, y: number, z: number): void;
  scroll_slice(viewport: number, delta: number): void;
  set_window_level(center: number, width: number): void;
  orbit(dx: number, dy: number): void;
  pan(dx: number, dy: number): void;
  zoom(factor: number): void;
  set_blend_mode(mode: number): void;
  set_thick_slab(viewport: number, thickness: number, projection: number): void;
  set_volume_preset(name: string): void;
  reset(): void;
  destroy(): void;
}

export function decode_dicom_pixels(bytes: Uint8Array): Int16Array;
