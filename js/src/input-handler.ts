import type { InputTool, ViewportId } from "./types.js";
import type { StackViewer, Viewer } from "./viewer.js";

const WL_SENSITIVITY = 1.0;
const PAN_SENSITIVITY = 1.0;
const ZOOM_SENSITIVITY = 0.005;
const SCROLL_SENSITIVITY = 1.0;

type ViewerLike = Viewer | StackViewer;

interface Dragging {
  startX: number;
  startY: number;
  button: number;
}

function hasMethod(viewer: ViewerLike, name: string): boolean {
  return typeof (viewer as unknown as Record<string, unknown>)[name] === "function";
}

/**
 * Optional built-in input handler for slice viewports.
 *
 * Binds pointer and wheel events to a canvas and translates them into
 * Viewer API calls based on the active tool. Supports left/middle/right
 * mouse button differentiation:
 * - **Left button**: active tool (windowLevel, pan, zoom, crosshair, scroll)
 * - **Middle button**: pan
 * - **Right button**: zoom
 * - **Wheel**: scroll (shift+wheel → zoom)
 */
export class InputHandler {
  readonly #canvas: HTMLCanvasElement;
  readonly #viewer: ViewerLike;
  readonly #viewport: ViewportId;
  #activeTool: InputTool = "windowLevel";
  #dragging: Dragging | null = null;
  readonly #abortController = new AbortController();
  #renderScheduled = false;

  constructor(
    canvas: HTMLCanvasElement,
    viewer: ViewerLike,
    viewport: ViewportId = "axial",
  ) {
    this.#canvas = canvas;
    this.#viewer = viewer;
    this.#viewport = viewport;
    this.#bind();
  }

  get activeTool(): InputTool {
    return this.#activeTool;
  }

  setActiveTool(tool: InputTool): void {
    this.#activeTool = tool;
  }

  destroy(): void {
    this.#abortController.abort();
    this.#dragging = null;
  }

  #bind(): void {
    const signal = this.#abortController.signal;
    const canvas = this.#canvas;

    canvas.addEventListener(
      "pointerdown",
      (e) => {
        e.preventDefault();
        canvas.setPointerCapture(e.pointerId);
        this.#dragging = { startX: e.clientX, startY: e.clientY, button: e.button };
      },
      { signal },
    );

    canvas.addEventListener(
      "pointermove",
      (e) => {
        if (!this.#dragging) return;
        const dx = e.clientX - this.#dragging.startX;
        const dy = e.clientY - this.#dragging.startY;
        this.#dragging.startX = e.clientX;
        this.#dragging.startY = e.clientY;
        this.#handleDrag(dx, dy, this.#dragging.button);
      },
      { signal },
    );

    canvas.addEventListener(
      "pointerup",
      (e) => {
        canvas.releasePointerCapture(e.pointerId);
        this.#dragging = null;
      },
      { signal },
    );

    canvas.addEventListener(
      "pointercancel",
      (e) => {
        canvas.releasePointerCapture(e.pointerId);
        this.#dragging = null;
      },
      { signal },
    );

    canvas.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        if (e.shiftKey) {
          const factor = 1.0 - e.deltaY * ZOOM_SENSITIVITY;
          if (hasMethod(this.#viewer, "zoom")) {
            (this.#viewer as Viewer).zoom(factor);
          }
        } else {
          const delta = (e.deltaY > 0 ? 1 : -1) * SCROLL_SENSITIVITY;
          this.#scroll(delta);
        }
        this.#scheduleRender();
      },
      { signal, passive: false },
    );

    canvas.addEventListener("contextmenu", (e) => e.preventDefault(), { signal });
  }

  #handleDrag(dx: number, dy: number, button: number): void {
    if (button === 1) {
      this.#pan(dx, dy);
    } else if (button === 2) {
      const factor = 1.0 - dy * ZOOM_SENSITIVITY;
      if (hasMethod(this.#viewer, "zoom")) {
        (this.#viewer as Viewer).zoom(factor);
      }
    } else {
      this.#handleToolDrag(dx, dy);
    }
    this.#scheduleRender();
  }

  #handleToolDrag(dx: number, dy: number): void {
    switch (this.#activeTool) {
      case "windowLevel": {
        const viewer = this.#viewer;
        // Approximate W/L adjustment — caller should maintain center/width state
        // for proper behavior; this provides a reasonable default interaction.
        viewer.setWindowLevel(dx * WL_SENSITIVITY, dy * WL_SENSITIVITY);
        break;
      }
      case "pan":
        this.#pan(dx, dy);
        break;
      case "zoom": {
        const factor = 1.0 - dy * ZOOM_SENSITIVITY;
        if (hasMethod(this.#viewer, "zoom")) {
          (this.#viewer as Viewer).zoom(factor);
        }
        break;
      }
      case "crosshair":
        // Crosshair requires world-space conversion — viewer-specific, not handled here
        break;
      case "scroll":
        this.#scroll(dy > 0 ? 1 : dy < 0 ? -1 : 0);
        break;
    }
  }

  #pan(dx: number, dy: number): void {
    if (hasMethod(this.#viewer, "pan")) {
      (this.#viewer as Viewer).pan(dx * PAN_SENSITIVITY, dy * PAN_SENSITIVITY);
    }
  }

  #scroll(delta: number): void {
    const viewer = this.#viewer;
    if (hasMethod(viewer, "scrollSlice") && !hasMethod(viewer, "orbit")) {
      // StackViewer: scrollSlice(delta)
      (viewer as StackViewer).scrollSlice(delta);
    } else if (hasMethod(viewer, "scrollSlice")) {
      // Viewer: scrollSlice(viewport, delta)
      (viewer as Viewer).scrollSlice(this.#viewport, delta);
    }
  }

  #scheduleRender(): void {
    if (this.#renderScheduled) return;
    this.#renderScheduled = true;
    requestAnimationFrame(() => {
      this.#renderScheduled = false;
      this.#viewer.render();
    });
  }
}

/**
 * Input handler specialized for the 3D volume viewport.
 *
 * - **Left drag**: orbit (rotate camera)
 * - **Middle drag**: pan
 * - **Right drag**: zoom
 * - **Wheel**: zoom
 */
export class VolumeInputHandler {
  readonly #canvas: HTMLCanvasElement;
  readonly #viewer: Viewer;
  #dragging: Dragging | null = null;
  readonly #abortController = new AbortController();
  #renderScheduled = false;

  constructor(canvas: HTMLCanvasElement, viewer: Viewer) {
    this.#canvas = canvas;
    this.#viewer = viewer;
    this.#bind();
  }

  destroy(): void {
    this.#abortController.abort();
    this.#dragging = null;
  }

  #bind(): void {
    const signal = this.#abortController.signal;
    const canvas = this.#canvas;

    canvas.addEventListener(
      "pointerdown",
      (e) => {
        e.preventDefault();
        canvas.setPointerCapture(e.pointerId);
        this.#dragging = { startX: e.clientX, startY: e.clientY, button: e.button };
      },
      { signal },
    );

    canvas.addEventListener(
      "pointermove",
      (e) => {
        if (!this.#dragging) return;
        const dx = e.clientX - this.#dragging.startX;
        const dy = e.clientY - this.#dragging.startY;
        this.#dragging.startX = e.clientX;
        this.#dragging.startY = e.clientY;

        if (this.#dragging.button === 0) {
          this.#viewer.orbit(dx, dy);
        } else if (this.#dragging.button === 1) {
          this.#viewer.pan(dx * PAN_SENSITIVITY, dy * PAN_SENSITIVITY);
        } else if (this.#dragging.button === 2) {
          const factor = 1.0 - dy * ZOOM_SENSITIVITY;
          this.#viewer.zoom(factor);
        }
        this.#scheduleRender();
      },
      { signal },
    );

    canvas.addEventListener(
      "pointerup",
      (e) => {
        canvas.releasePointerCapture(e.pointerId);
        this.#dragging = null;
      },
      { signal },
    );

    canvas.addEventListener(
      "pointercancel",
      (e) => {
        canvas.releasePointerCapture(e.pointerId);
        this.#dragging = null;
      },
      { signal },
    );

    canvas.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        const factor = 1.0 - e.deltaY * ZOOM_SENSITIVITY;
        this.#viewer.zoom(factor);
        this.#scheduleRender();
      },
      { signal, passive: false },
    );

    canvas.addEventListener("contextmenu", (e) => e.preventDefault(), { signal });
  }

  #scheduleRender(): void {
    if (this.#renderScheduled) return;
    this.#renderScheduled = true;
    requestAnimationFrame(() => {
      this.#renderScheduled = false;
      this.#viewer.render();
    });
  }
}
