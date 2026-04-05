import { StackViewer, Viewer } from "./viewer.js";
import type {
  DICOMwebLoaderOptions,
  ProgressCallback,
  SeriesParams,
  VolumeGeometry,
  WasmSource,
} from "./types.js";

type DicomJsonValue = string | number;

type DicomJsonElement = {
  vr: string;
  Value?: DicomJsonValue[];
};

type DicomJsonInstance = Record<string, DicomJsonElement | undefined>;

type InstanceMetadata = {
  sopInstanceUid: string;
  instanceNumber: number;
  rows: number;
  columns: number;
  pixelSpacing?: [number, number];
  sliceThickness?: number;
  imagePosition?: [number, number, number];
  imageOrientation?: [
    [number, number, number],
    [number, number, number],
  ];
  numberOfFrames: number;
};

type DecodeRequest = {
  type: "decode";
  jobId: number;
  sliceIndex: number;
  wasmUrl?: string;
  bytes: ArrayBufferLike;
};

type DecodeResponse =
  | {
      type: "decoded";
      jobId: number;
      sliceIndex: number;
      pixels: Int16Array;
    }
  | {
      type: "error";
      jobId: number;
      message: string;
    };

type PendingJob = {
  resolve: (value: { sliceIndex: number; pixels: Int16Array }) => void;
  reject: (reason?: unknown) => void;
};

export class DICOMwebLoader {
  readonly #options: DICOMwebLoaderOptions;
  #progressCallback: ProgressCallback | null = null;
  #abortController: AbortController | null = null;

  constructor(options: DICOMwebLoaderOptions) {
    this.#options = {
      ...options,
      concurrency: Math.max(1, options.concurrency ?? 4),
      decodeWorkers: Math.max(0, options.decodeWorkers ?? 0),
    };
  }

  onProgress(callback: ProgressCallback): void {
    this.#progressCallback = callback;
  }

  abort(): void {
    this.#abortController?.abort();
    this.#abortController = null;
  }

  async loadSeries(viewer: Viewer | StackViewer, params: SeriesParams): Promise<void> {
    const renderDuringLoad = this.#options.renderDuringLoad !== false;
    const controller = new AbortController();
    this.#abortController = controller;
    const metadataUrl = [
      trimRoot(this.#options.wadoRoot),
      "studies",
      encodeURIComponent(params.studyUid),
      "series",
      encodeURIComponent(params.seriesUid),
      "metadata",
    ].join("/");

    const metadataJson = (await this.#fetchJson(metadataUrl, controller.signal)) as DicomJsonInstance[];
    const instances = parseSeriesMetadata(metadataJson);
    const geometry = deriveGeometry(instances);
    viewer.prepareVolume(geometry);

    const pool = this.#createDecodeWorkerPool();
    const total = instances.length;
    let loaded = 0;
    let nextIndex = 0;
    let renderScheduled = false;
    const scheduleRender = (): void => {
      if (!renderDuringLoad || renderScheduled) {
        return;
      }
      renderScheduled = true;
      const requestFrame =
        typeof requestAnimationFrame === "function"
          ? requestAnimationFrame
          : (callback: FrameRequestCallback) => {
              callback(0);
              return 0;
            };
      requestFrame(() => {
        renderScheduled = false;
        try {
          viewer.render();
        } catch {
          // Viewer may have been destroyed between scheduling and callback
        }
      });
    };

    try {
      const workers = Array.from(
        { length: Math.min(this.#options.concurrency ?? 1, total) },
        async () => {
          while (true) {
            const sliceIndex = nextIndex;
            nextIndex += 1;
            if (sliceIndex >= total) {
              return;
            }

            const instance = instances[sliceIndex];
            const instanceUrl = [
              trimRoot(this.#options.wadoRoot),
              "studies",
              encodeURIComponent(params.studyUid),
              "series",
              encodeURIComponent(params.seriesUid),
              "instances",
              encodeURIComponent(instance.sopInstanceUid),
            ].join("/");
            const bytes = await this.#fetchBytes(instanceUrl, controller.signal);
            if (pool) {
              const decoded = await pool.decode(sliceIndex, bytes);
              viewer.feedPixelSlice(decoded.sliceIndex, decoded.pixels);
            } else {
              viewer.feedDicomSlice(sliceIndex, bytes);
            }
            loaded += 1;
            this.#progressCallback?.(loaded, total);
            scheduleRender();
          }
        },
      );

      await Promise.all(workers);
      try {
        viewer.render();
      } catch {
        // Viewer may have been destroyed during load
      }
    } finally {
      pool?.destroy();
      if (this.#abortController === controller) {
        this.#abortController = null;
      }
    }
  }

  async #fetchJson(url: string, signal: AbortSignal): Promise<unknown> {
    const response = await this.#fetch(url, signal, "application/dicom+json");
    return response.json();
  }

  async #fetchBytes(url: string, signal: AbortSignal): Promise<Uint8Array> {
    const response = await this.#fetch(
      url,
      signal,
      'multipart/related; type="application/dicom"',
    );
    const contentType = response.headers.get("Content-Type") ?? "";
    if (contentType.includes("multipart/related")) {
      return extractMultipartDicom(new Uint8Array(await response.arrayBuffer()), contentType);
    }
    // Fallback: server returned a single-part DICOM response
    return new Uint8Array(await response.arrayBuffer());
  }

  async #fetch(url: string, signal: AbortSignal, accept: string): Promise<Response> {
    const fetchImpl = this.#options.fetch ?? globalThis.fetch.bind(globalThis);
    const headers = new Headers(this.#options.headers);
    headers.set("Accept", accept);
    const response = await fetchImpl(url, {
      method: "GET",
      headers,
      signal,
    });
    if (!response.ok) {
      throw new Error(`HTTP ${response.status} while fetching ${url}`);
    }
    return response;
  }

  #createDecodeWorkerPool(): DecodeWorkerPool | null {
    const requested = this.#options.decodeWorkers ?? 0;
    if (requested <= 0 || typeof Worker === "undefined") {
      return null;
    }
    const wasmUrl = normalizeWorkerWasmUrl(this.#options.wasmUrl);
    return new DecodeWorkerPool(requested, wasmUrl);
  }
}

class DecodeWorkerPool {
  readonly #workers: Worker[];
  readonly #pending = new Map<number, PendingJob>();
  readonly #workerUrl = new URL("./decode-worker.js", import.meta.url);
  #nextWorkerIndex = 0;
  #nextJobId = 1;

  constructor(size: number, wasmUrl?: string) {
    this.#workers = Array.from({ length: size }, () => {
      const worker = new Worker(this.#workerUrl, { type: "module" });
      worker.addEventListener("message", (event: MessageEvent<DecodeResponse>) => {
        const message = event.data;
        const pending = this.#pending.get(message.jobId);
        if (!pending) {
          return;
        }
        this.#pending.delete(message.jobId);
        if (message.type === "decoded") {
          pending.resolve({
            sliceIndex: message.sliceIndex,
            pixels: message.pixels,
          });
        } else {
          pending.reject(new Error(message.message));
        }
      });
      worker.addEventListener("error", (event) => {
        for (const [jobId, pending] of this.#pending) {
          this.#pending.delete(jobId);
          pending.reject(event.error ?? new Error(event.message));
        }
      });
      (worker as Worker & { __dicomviewWasmUrl?: string }).__dicomviewWasmUrl = wasmUrl;
      return worker;
    });
  }

  decode(sliceIndex: number, bytes: Uint8Array): Promise<{ sliceIndex: number; pixels: Int16Array }> {
    const worker = this.#workers[this.#nextWorkerIndex];
    this.#nextWorkerIndex = (this.#nextWorkerIndex + 1) % this.#workers.length;
    const jobId = this.#nextJobId;
    this.#nextJobId += 1;
    const buffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
    const message: DecodeRequest = {
      type: "decode",
      jobId,
      sliceIndex,
      wasmUrl: (worker as Worker & { __dicomviewWasmUrl?: string }).__dicomviewWasmUrl,
      bytes: buffer,
    };
    worker.postMessage(message, [buffer]);
    return new Promise((resolve, reject) => {
      this.#pending.set(jobId, { resolve, reject });
    });
  }

  destroy(): void {
    for (const worker of this.#workers) {
      worker.terminate();
    }
    this.#workers.length = 0;
    this.#pending.clear();
  }
}

function parseSeriesMetadata(metadata: DicomJsonInstance[]): InstanceMetadata[] {
  const instances = metadata.map(parseInstanceMetadata);
  instances.sort(compareInstances);
  return instances;
}

function parseInstanceMetadata(instance: DicomJsonInstance): InstanceMetadata {
  const sopInstanceUid = firstString(instance, "00080018");
  if (!sopInstanceUid) {
    throw new Error("DICOMweb metadata is missing SOP Instance UID");
  }
  const rows = firstNumber(instance, "00280010");
  const columns = firstNumber(instance, "00280011");
  if (rows === undefined || columns === undefined) {
    throw new Error("DICOMweb metadata is missing image dimensions");
  }
  const orientation = firstNumberArray(instance, "00200037", 6);

  return {
    sopInstanceUid,
    instanceNumber: firstNumber(instance, "00200013") ?? 0,
    rows,
    columns,
    pixelSpacing: pairNumberArray(instance, "00280030"),
    sliceThickness: firstNumber(instance, "00180050"),
    imagePosition: tripletNumberArray(instance, "00200032"),
    imageOrientation: orientation
      ? [
          [orientation[0], orientation[1], orientation[2]],
          [orientation[3], orientation[4], orientation[5]],
        ]
      : undefined,
    numberOfFrames: firstNumber(instance, "00280008") ?? 1,
  };
}

function deriveGeometry(instances: InstanceMetadata[]): VolumeGeometry {
  if (instances.length === 0) {
    throw new Error("Series metadata is empty");
  }
  if (instances.some((instance) => instance.numberOfFrames !== 1)) {
    throw new Error("Multi-frame DICOMweb metadata is not yet supported");
  }
  const first = instances[0];
  if (instances.some((instance) => instance.rows !== first.rows || instance.columns !== first.columns)) {
    throw new Error("Series contains inconsistent frame dimensions");
  }

  const rowDirection = first.imageOrientation?.[0] ?? [1, 0, 0];
  const columnDirection = first.imageOrientation?.[1] ?? [0, 1, 0];
  const normal = normalize(cross(rowDirection, columnDirection));
  const pixelSpacing = first.pixelSpacing ?? [1, 1];
  const sliceSpacing =
    projectedSliceSpacing(instances) ?? first.sliceThickness ?? 1;

  return {
    dimensions: [first.columns, first.rows, instances.length],
    spacing: [pixelSpacing[1], pixelSpacing[0], sliceSpacing],
    origin: first.imagePosition ?? [0, 0, 0],
    direction: [rowDirection, columnDirection, normal],
  };
}

function projectedSliceSpacing(instances: InstanceMetadata[]): number | undefined {
  if (instances.length < 2) {
    return undefined;
  }
  const row = instances[0].imageOrientation?.[0];
  const col = instances[0].imageOrientation?.[1];
  const first = instances[0].imagePosition;
  const second = instances[1].imagePosition;
  if (!row || !col || !first || !second) {
    return undefined;
  }
  const normal = normalize(cross(row, col));
  return Math.abs(dot(normal, subtract(second, first)));
}

function compareInstances(left: InstanceMetadata, right: InstanceMetadata): number {
  const row = left.imageOrientation?.[0];
  const col = left.imageOrientation?.[1];
  if (left.imagePosition && right.imagePosition && row && col) {
    const normal = normalize(cross(row, col));
    const leftDistance = dot(normal, left.imagePosition);
    const rightDistance = dot(normal, right.imagePosition);
    return leftDistance - rightDistance;
  }

  return (
    left.instanceNumber - right.instanceNumber ||
    left.sopInstanceUid.localeCompare(right.sopInstanceUid)
  );
}

function tripletNumberArray(
  instance: DicomJsonInstance,
  tag: string,
): [number, number, number] | undefined {
  const values = firstNumberArray(instance, tag, 3);
  return values ? [values[0], values[1], values[2]] : undefined;
}

function pairNumberArray(
  instance: DicomJsonInstance,
  tag: string,
): [number, number] | undefined {
  const values = firstNumberArray(instance, tag, 2);
  return values ? [values[0], values[1]] : undefined;
}

function firstNumberArray(
  instance: DicomJsonInstance,
  tag: string,
  length: number,
): number[] | undefined {
  const values = instance[tag]?.Value;
  if (!values || values.length < length) {
    return undefined;
  }
  return values.slice(0, length).map((value) => {
    if (typeof value === "number") {
      return value;
    }
    const parsed = Number.parseFloat(value);
    if (!Number.isFinite(parsed)) {
      throw new Error(`Tag ${tag} contains a non-numeric value`);
    }
    return parsed;
  });
}

function firstNumber(instance: DicomJsonInstance, tag: string): number | undefined {
  const value = instance[tag]?.Value?.[0];
  if (value === undefined) {
    return undefined;
  }
  if (typeof value === "number") {
    return value;
  }
  const parsed = Number.parseFloat(value);
  if (!Number.isFinite(parsed)) {
    throw new Error(`Tag ${tag} contains a non-numeric value`);
  }
  return parsed;
}

function firstString(instance: DicomJsonInstance, tag: string): string | undefined {
  const value = instance[tag]?.Value?.[0];
  return typeof value === "string" ? value : undefined;
}

function trimRoot(root: string): string {
  return root.replace(/\/+$/, "");
}

function normalizeWorkerWasmUrl(wasmUrl: WasmSource | undefined): string | undefined {
  if (typeof wasmUrl === "string") {
    return wasmUrl;
  }
  if (wasmUrl instanceof URL) {
    return wasmUrl.toString();
  }
  return undefined;
}

function subtract(
  left: [number, number, number],
  right: [number, number, number],
): [number, number, number] {
  return [left[0] - right[0], left[1] - right[1], left[2] - right[2]];
}

function cross(
  left: [number, number, number],
  right: [number, number, number],
): [number, number, number] {
  return [
    left[1] * right[2] - left[2] * right[1],
    left[2] * right[0] - left[0] * right[2],
    left[0] * right[1] - left[1] * right[0],
  ];
}

function dot(
  left: [number, number, number],
  right: [number, number, number],
): number {
  return left[0] * right[0] + left[1] * right[1] + left[2] * right[2];
}

function normalize(vector: [number, number, number]): [number, number, number] {
  const length = Math.hypot(vector[0], vector[1], vector[2]);
  if (length === 0) {
    return [0, 0, 1];
  }
  return [vector[0] / length, vector[1] / length, vector[2] / length];
}

/**
 * Extract the DICOM file bytes from a WADO-RS multipart/related response.
 *
 * The response body contains one or more parts separated by a MIME boundary.
 * We extract the first part (single-instance retrieval returns exactly one).
 */
function extractMultipartDicom(body: Uint8Array, contentType: string): Uint8Array {
  const boundaryMatch = contentType.match(/boundary=([^\s;]+)/);
  if (!boundaryMatch) {
    // No boundary found — assume the body is the raw DICOM bytes
    return body;
  }
  const boundary = boundaryMatch[1].replace(/^"(.*)"$/, "$1");
  const boundaryBytes = new TextEncoder().encode("--" + boundary);

  // Find the first boundary
  const firstBoundary = indexOfBytes(body, boundaryBytes, 0);
  if (firstBoundary === -1) {
    return body;
  }

  // After the boundary line, skip until we find \r\n\r\n (end of part headers)
  const headerStart = firstBoundary + boundaryBytes.length;
  const headerEnd = indexOfBytes(body, new Uint8Array([0x0d, 0x0a, 0x0d, 0x0a]), headerStart);
  if (headerEnd === -1) {
    return body;
  }
  const partStart = headerEnd + 4;

  // Find the next boundary (or end boundary) — the part data ends 2 bytes before it (\r\n)
  const nextBoundary = indexOfBytes(body, boundaryBytes, partStart);
  const partEnd = nextBoundary === -1 ? body.length : nextBoundary - 2;

  return body.subarray(partStart, partEnd);
}

function indexOfBytes(haystack: Uint8Array, needle: Uint8Array, offset: number): number {
  const end = haystack.length - needle.length;
  outer: for (let i = offset; i <= end; i++) {
    for (let j = 0; j < needle.length; j++) {
      if (haystack[i + j] !== needle[j]) {
        continue outer;
      }
    }
    return i;
  }
  return -1;
}
