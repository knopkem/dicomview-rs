import initWasm, { decode_dicom_pixels } from "../wasm/dicomview_wasm.js";

type DecodeRequest = {
  type: "decode";
  jobId: number;
  sliceIndex: number;
  wasmUrl?: string;
  bytes: ArrayBuffer;
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

let wasmReady: ReturnType<typeof initWasm> | null = null;

self.addEventListener("message", async (event: MessageEvent<DecodeRequest>) => {
  const message = event.data;
  if (message.type !== "decode") {
    return;
  }

  try {
    if (!wasmReady) {
      wasmReady = initWasm(message.wasmUrl);
    }
    await wasmReady;
    const pixels = decode_dicom_pixels(new Uint8Array(message.bytes));
    const response: DecodeResponse = {
      type: "decoded",
      jobId: message.jobId,
      sliceIndex: message.sliceIndex,
      pixels,
    };
    self.postMessage(response, [pixels.buffer]);
  } catch (error) {
    const response: DecodeResponse = {
      type: "error",
      jobId: message.jobId,
      message: error instanceof Error ? error.message : String(error),
    };
    self.postMessage(response);
  }
});

export {};
