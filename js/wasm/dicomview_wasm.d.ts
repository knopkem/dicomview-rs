/* tslint:disable */
/* eslint-disable */

/**
 * One JS-visible viewer instance managing four canvases.
 */
export class Viewer {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Creates a new viewer bound to four canvas elements.
     */
    static create(config: any): Promise<Viewer>;
    /**
     * Explicitly destroys the viewer and releases its resources.
     */
    destroy(): void;
    /**
     * Decodes one DICOM Part 10 payload and uploads its frame data.
     */
    feed_dicom_slice(z_index: number, bytes: Uint8Array): void;
    /**
     * Uploads one already-decoded signed 16-bit slice.
     */
    feed_pixel_slice(z_index: number, pixels: Int16Array): void;
    /**
     * Returns the current loading progress in `[0, 1]`.
     */
    loading_progress(): number;
    /**
     * Orbits the 3D volume camera.
     */
    orbit(dx: number, dy: number): void;
    /**
     * Pans the 3D volume camera.
     */
    pan(dx: number, dy: number): void;
    /**
     * Prepares an empty volume with the provided geometry object.
     */
    prepare_volume(geometry: any): void;
    /**
     * Renders all four canvases.
     */
    render(): void;
    /**
     * Resets all viewport state back to defaults.
     */
    reset(): void;
    /**
     * Scrolls one of the three slice viewports.
     */
    scroll_slice(viewport: number, delta: number): void;
    /**
     * Selects the active volume blend mode.
     */
    set_blend_mode(mode: number): void;
    /**
     * Updates the shared MPR crosshair in world coordinates.
     */
    set_crosshair(x: number, y: number, z: number): void;
    /**
     * Configures thick-slab rendering for one slice viewport.
     */
    set_thick_slab(viewport: number, thickness: number, projection: number): void;
    /**
     * Switches to one of the built-in volume presets.
     */
    set_volume_preset(name: string): void;
    /**
     * Applies one window/level setting to all viewports.
     */
    set_window_level(center: number, width: number): void;
    /**
     * Zooms the 3D volume camera.
     */
    zoom(factor: number): void;
}

/**
 * A simple WADO-RS series loader with progress reporting and abort support.
 */
export class WadoLoader {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Aborts any active in-flight requests.
     */
    abort(): void;
    /**
     * Loads a single-frame DICOM series through WADO-RS metadata and instance retrieval.
     */
    load_series(viewer: Viewer, wado_root: string, study_uid: string, series_uid: string): Promise<void>;
    /**
     * Returns how many slices have been loaded so far.
     */
    loaded(): number;
    /**
     * Creates a new loader.
     */
    constructor();
    /**
     * Returns the total number of slices expected.
     */
    total(): number;
}

/**
 * Decodes one single-frame DICOM Part 10 payload into signed 16-bit pixels.
 */
export function decode_dicom_pixels(bytes: Uint8Array): Int16Array;

/**
 * Initializes the panic hook used by the wasm facade.
 */
export function init(): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_viewer_free: (a: number, b: number) => void;
    readonly viewer_create: (a: any) => any;
    readonly viewer_destroy: (a: number) => void;
    readonly viewer_feed_dicom_slice: (a: number, b: number, c: number, d: number) => [number, number];
    readonly viewer_feed_pixel_slice: (a: number, b: number, c: number, d: number) => [number, number];
    readonly viewer_loading_progress: (a: number) => number;
    readonly viewer_orbit: (a: number, b: number, c: number) => [number, number];
    readonly viewer_pan: (a: number, b: number, c: number) => [number, number];
    readonly viewer_prepare_volume: (a: number, b: any) => [number, number];
    readonly viewer_render: (a: number) => [number, number];
    readonly viewer_reset: (a: number) => [number, number];
    readonly viewer_scroll_slice: (a: number, b: number, c: number) => [number, number];
    readonly viewer_set_blend_mode: (a: number, b: number) => [number, number];
    readonly viewer_set_crosshair: (a: number, b: number, c: number, d: number) => [number, number];
    readonly viewer_set_thick_slab: (a: number, b: number, c: number, d: number) => [number, number];
    readonly viewer_set_volume_preset: (a: number, b: number, c: number) => [number, number];
    readonly viewer_set_window_level: (a: number, b: number, c: number) => [number, number];
    readonly viewer_zoom: (a: number, b: number) => [number, number];
    readonly __wbg_wadoloader_free: (a: number, b: number) => void;
    readonly wadoloader_abort: (a: number) => void;
    readonly wadoloader_load_series: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => any;
    readonly wadoloader_loaded: (a: number) => number;
    readonly wadoloader_new: () => number;
    readonly wadoloader_total: (a: number) => number;
    readonly decode_dicom_pixels: (a: number, b: number) => [number, number, number, number];
    readonly init: () => void;
    readonly wasm_bindgen__convert__closures_____invoke__h77f088fde8c66f5c: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen__convert__closures_____invoke__h6a18030bf0401ad0: (a: number, b: number, c: any, d: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__h4c19ef90e3a2449e: (a: number, b: number, c: any) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_destroy_closure: (a: number, b: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
