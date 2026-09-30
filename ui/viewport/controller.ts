/**
 * Document viewport controller: drives the compositor from poll results,
 * handles pan/zoom input, and manages the tile display set.
 *
 * Interaction model (MASTER_PLAN.md §6, §10, §11):
 * - authoritative view state (scroll, zoom, layout) lives in Rust; the UI
 *   mirrors it and applies display-only deltas during gestures, then snaps
 *   to the authoritative values from each poll
 * - zoom is focal-point centered (cursor / pinch midpoint) and goes through
 *   the Rust DocumentZoomController; during a gesture, existing tiles stay
 *   on screen scaled (no blanking) while the new quantized step renders
 * - predicted/gesture values never become document state
 */

import {
  pan as ipcPan,
  poll,
  requestThumbnails,
  rotate as ipcRotate,
  sendViewport,
  zoom as ipcZoom,
  type LayoutJson,
} from "../app/ipc";
import { app } from "../state/app";
import { Compositor } from "./compositor";
import { fetchTile } from "./tiles";

const TILE_PX = 256;
/** quantized ladder step → zoom (matches Rust zoom_for_step) */
function zoomForStep(step: number): number {
  return Math.pow(2, step / 8);
}

interface ViewDoc {
  id: number;
  layout: LayoutJson | null;
  layoutRev: number;
  scroll: { x: number; y: number };
  zoom: number;
  zoomStep: number;
  activePage: number;
  /** display-only pan delta applied this frame (reconciled by poll) */
  localPan: { x: number; y: number };
  lastSentScroll: { x: number; y: number };
  lastSentVel: number;
  /** decaying scroll-velocity estimate (view px per frame) */
  dragVel: number;
  /** shown tiles for the current draw: page → list */
  pendingFetches: Set<string>;
  readyKeys: Set<string>;
  lastPollMs: number;
}

export class ViewportController {
  private comp: Compositor | null = null;
  private canvas: HTMLCanvasElement | null = null;
  private root: HTMLElement | null = null;
  private empty: HTMLElement | null = null;
  private doc: ViewDoc | null = null;
  private raf = 0;
  private dragging = false;
  private dragLast = { x: 0, y: 0 };
  private dragVel = 0;
  private pinch: { dist: number; cx: number; cy: number } | null = null;
  private onStats?: (stats: { cacheTiles: number; cacheBytes: number; renderedTotal: number; staleDropped: number; activePage: number }) => void;
  private onLayout?: (layout: LayoutJson) => void;
  private onZoomChanged?: (zoom: number) => void;
  private thumbWidth = 96;
  private thumbsRequested = new Set<string>();

  init(root: HTMLElement, hooks: {
    onStats?: ViewportController["onStats"];
    onLayout?: ViewportController["onLayout"];
    onZoomChanged?: ViewportController["onZoomChanged"];
  }): void {
    this.root = root;
    this.onStats = hooks.onStats;
    this.onLayout = hooks.onLayout;
    this.onZoomChanged = hooks.onZoomChanged;

    this.canvas = document.createElement("canvas");
    this.canvas.className = "viewport-canvas";
    this.canvas.setAttribute("aria-hidden", "true");
    root.appendChild(this.canvas);
    this.comp = new Compositor(this.canvas);

    const empty = document.createElement("div");
    empty.className = "viewport-empty";
    const title = document.createElement("div");
    title.className = "title";
    title.textContent = "Open a document to begin";
    const hint = document.createElement("div");
    const kbd = document.createElement("kbd");
    kbd.textContent = "Ctrl+O";
    hint.append(kbd, document.createTextNode(" to open a PDF — or press "));
    const kbd2 = document.createElement("kbd");
    kbd2.textContent = "Ctrl+K";
    hint.appendChild(kbd2);
    const fileInfo = document.createElement("div");
    fileInfo.className = "viewport-file-info";
    empty.append(title, hint, fileInfo);
    root.appendChild(empty);
    this.empty = empty;

    app.subscribe((s) => {
      const f = s.lastOpenedFile;
      fileInfo.textContent = f
        ? `${f.name} — ${formatBytes(f.size_bytes)}`
        : "";
      empty.style.display = f ? "none" : "flex";
    });

    this.bindInput();
    this.loop();
  }

  openDocument(id: number): void {
    this.doc = {
      id,
      layout: null,
      layoutRev: -1,
      scroll: { x: 0, y: 0 },
      zoom: 1,
      zoomStep: 0,
      activePage: 0,
      localPan: { x: 0, y: 0 },
      lastSentScroll: { x: -1, y: -1 },
      lastSentVel: 0,
      dragVel: 0,
      pendingFetches: new Set(),
      readyKeys: new Set(),
      lastPollMs: 0,
    };
    this.thumbsRequested = new Set();
    this.resizeCanvas();
  }

  closeDocument(): void {
    this.doc = null;
  }

  hasDocument(id: number): boolean {
    return this.doc?.id === id;
  }

  /** Programmatic zoom entry (buttons, shortcuts, palette). */
  zoomBy(factor: number): void {
    const d = this.doc;
    if (!d || !this.canvas) return;
    const rect = this.canvas.getBoundingClientRect();
    void ipcZoom(d.id, factor, (rect.width / 2) * devicePixelRatio, (rect.height / 2) * devicePixelRatio);
  }

  zoomAbsolute(target: number): void {
    const d = this.doc;
    if (!d || !this.canvas) return;
    const rect = this.canvas.getBoundingClientRect();
    void ipcZoom(d.id, 1, (rect.width / 2) * devicePixelRatio, (rect.height / 2) * devicePixelRatio, target);
  }

  rotate(quarterTurns: number): void {
    const d = this.doc;
    if (d) void ipcRotate(d.id, quarterTurns);
  }

  /** Scrolls the view to keep a page visible (outline/pages navigation). */
  async gotoPage(page: number): Promise<void> {
    const d = this.doc;
    if (!d) return;
    const { gotoPage: ipcGoto } = await import("../app/ipc");
    const target = await ipcGoto(d.id, page);
    d.scroll = { x: target[0], y: target[1] };
    d.localPan = { x: 0, y: 0 };
  }

  // ---- input ----

  private bindInput(): void {
    const root = this.root!;
    root.style.touchAction = "none";

    root.addEventListener("wheel", (e) => {
      const d = this.doc;
      if (!d) return;
      e.preventDefault();
      const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
      const px = (e.clientX - rect.left) * devicePixelRatio;
      const py = (e.clientY - rect.top) * devicePixelRatio;
      if (e.ctrlKey) {
        // focal zoom (mouse ctrl+wheel or synthesized trackpad pinch)
        const factor = Math.exp(-e.deltaY * 0.0022);
        void ipcZoom(d.id, factor, px, py);
        // display-only immediate feedback; poll reconciles
        d.zoom = clampZoom(d.zoom * factor);
      } else {
        const dx = e.deltaX;
        const dy = e.deltaY;
        d.localPan.x += dx * devicePixelRatio;
        d.localPan.y += dy * devicePixelRatio;
        d.dragVel = dy * devicePixelRatio;
        void ipcPan(d.id, dx * devicePixelRatio, dy * devicePixelRatio);
      }
    }, { passive: false });

    root.addEventListener("pointerdown", (e) => {
      if (e.pointerType === "touch") {
        this.trackPinch(e);
        return;
      }
      if (e.button !== 0) return;
      this.dragging = true;
      this.dragLast = { x: e.clientX, y: e.clientY };
      root.setPointerCapture(e.pointerId);
    });

    root.addEventListener("pointermove", (e) => {
      const d = this.doc;
      if (!d) return;
      if (e.pointerType === "touch") {
        this.trackPinch(e);
        return;
      }
      if (!this.dragging) return;
      const dx = (e.clientX - this.dragLast.x) * devicePixelRatio;
      const dy = (e.clientY - this.dragLast.y) * devicePixelRatio;
      this.dragLast = { x: e.clientX, y: e.clientY };
      d.localPan.x -= dx;
      d.localPan.y -= dy;
      d.dragVel = -dy;
      void ipcPan(d.id, -dx, -dy);
    });

    const endDrag = () => {
      this.dragging = false;
      this.pinch = null;
    };
    root.addEventListener("pointerup", endDrag);
    root.addEventListener("pointercancel", endDrag);

    window.addEventListener("resize", () => this.resizeCanvas());
  }

  private pointers = new Map<number, { x: number; y: number }>();

  private trackPinch(e: PointerEvent): void {
    const d = this.doc;
    if (!d || !this.canvas) return;
    if (e.type === "pointerdown") {
      this.pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (this.pointers.size === 2) {
        const pts = [...this.pointers.values()];
        const a = pts[0]!;
        const b = pts[1]!;
        this.pinch = {
          dist: Math.hypot(a.x - b.x, a.y - b.y),
          cx: (a.x + b.x) / 2,
          cy: (a.y + b.y) / 2,
        };
      }
      return;
    }
    if (e.type === "pointermove" && this.pointers.has(e.pointerId)) {
      this.pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (this.pointers.size === 2 && this.pinch) {
        const pts = [...this.pointers.values()];
        const a = pts[0]!;
        const b = pts[1]!;
        const dist = Math.hypot(a.x - b.x, a.y - b.y);
        const factor = dist / this.pinch.dist;
        if (Math.abs(factor - 1) > 0.005) {
          const rect = this.canvas.getBoundingClientRect();
          void ipcZoom(
            d.id,
            factor,
            (this.pinch.cx - rect.left) * devicePixelRatio,
            (this.pinch.cy - rect.top) * devicePixelRatio,
          );
          d.zoom = clampZoom(d.zoom * factor);
          this.pinch.dist = dist;
        }
      }
    }
    if (e.type === "pointerup" || e.type === "pointercancel") {
      this.pointers.delete(e.pointerId);
      if (this.pointers.size < 2) this.pinch = null;
    }
  }

  private resizeCanvas(): void {
    if (!this.root || !this.canvas || !this.comp) return;
    const rect = this.root.getBoundingClientRect();
    this.comp.resize(rect.width, rect.height, devicePixelRatio);
  }

  // ---- main loop ----

  private loop = (): void => {
    this.raf = requestAnimationFrame(() => this.loop());
    this.tick();
  };

  private tick(): void {
    const d = this.doc;
    if (!d || !this.comp || !this.root || !this.canvas) return;
    this.resizeCanvas();

    // fire viewport update when our display state moved or size changed
    const displayed = {
      x: d.scroll.x + d.localPan.x,
      y: d.scroll.y + d.localPan.y,
    };
    const moved =
      Math.abs(displayed.x - d.lastSentScroll.x) > 0.5 ||
      Math.abs(displayed.y - d.lastSentScroll.y) > 0.5 ||
      Math.abs(d.dragVel - d.lastSentVel) > 0.5;
    if (moved) {
      const rect = this.root.getBoundingClientRect();
      void sendViewport({
        id: d.id,
        viewportW: rect.width * devicePixelRatio,
        viewportH: rect.height * devicePixelRatio,
        dpr: devicePixelRatio,
        scrollX: displayed.x,
        scrollY: displayed.y,
        velocityY: d.dragVel,
        interacting: this.dragging || this.pinch !== null,
      });
      d.lastSentScroll = { ...displayed };
      d.lastSentVel = d.dragVel;
      d.dragVel *= 0.5; // decaying velocity estimate
    }

    // poll authoritative state once per frame (cheap JSON)
    const now = performance.now();
    void poll(d.id, d.layoutRev)
      .then((res) => {
        d.lastPollMs = now;
        d.scroll = { x: res.scroll[0], y: res.scroll[1] };
        d.localPan = { x: 0, y: 0 };
        d.zoom = res.zoom;
        d.zoomStep = res.zoom_step;
        d.activePage = res.active_page;
        this.onZoomChanged?.(res.zoom);
        if (res.layout) {
          d.layout = res.layout;
          d.layoutRev = res.layout.rev;
          this.onLayout?.(res.layout);
        }
        for (const key of res.ready_tiles) d.readyKeys.add(key);
        this.onStats?.({
          cacheTiles: res.cache_tiles,
          cacheBytes: res.cache_bytes,
          renderedTotal: res.rendered_total,
          staleDropped: res.stale_dropped,
          activePage: res.active_page,
        });
        this.requestThumbsForVisible(res);
      })
      .catch(() => {
        /* bridge hiccup: keep drawing what we have */
      });

    // fetch tiles that became ready
    const zoomScale = d.zoom / zoomForStep(d.zoomStep);
    const toFetch: string[] = [];
    for (const key of d.readyKeys) {
      if (!this.comp!.hasTile(key) && !d.pendingFetches.has(key)) {
        toFetch.push(key);
      }
    }
    for (const key of toFetch.slice(0, 8)) {
      d.pendingFetches.add(key);
      void fetchTile(d.id, key).then((pixels) => {
        d.pendingFetches.delete(key);
        if (pixels) {
          this.comp!.putTile(key, d.zoomStep, pixels);
        }
      });
    }

    // build visible tile list with fallback to lower steps (no blanking)
    const tiles = this.visibleTiles(d, zoomScale);
    this.comp!.draw(d.layout, displayed, tiles);
  }

  /** Visible tile slots for the current viewport; falls back to lower
   *  zoom-step tiles (scaled) when the exact step is not ready yet. */
  private visibleTiles(
    d: ViewDoc,
    zoomScale: number,
  ): { page: number; x: number; y: number; w: number; h: number; key: string; zoomScale: number }[] {
    const out: { page: number; x: number; y: number; w: number; h: number; key: string; zoomScale: number }[] = [];
    const layout = d.layout;
    if (!layout || !this.root) return out;
    const rect = this.root.getBoundingClientRect();
    const vw = rect.width * devicePixelRatio;
    const vh = rect.height * devicePixelRatio;
    const vis = {
      x0: d.scroll.x + d.localPan.x,
      y0: d.scroll.y + d.localPan.y,
      x1: d.scroll.x + d.localPan.x + vw,
      y1: d.scroll.y + d.localPan.y + vh,
    };

    for (const page of layout.pages) {
      if (page.x + page.w < vis.x0 || page.x > vis.x1) continue;
      if (page.y + page.h < vis.y0 || page.y > vis.y1) continue;
      const gridW = Math.ceil(page.w / (TILE_PX * zoomScale));
      const gridH = Math.ceil(page.h / (TILE_PX * zoomScale));
      for (let ty = 0; ty < gridH; ty++) {
        for (let tx = 0; tx < gridW; tx++) {
          // tile slot in page space at the *current* zoom
          const slotW = TILE_PX * zoomScale;
          const slotH = TILE_PX * zoomScale;
          const sx = tx * slotW;
          const sy = ty * slotH;
          if (page.x + sx > vis.x1 || page.x + sx + slotW < vis.x0) continue;
          if (page.y + sy > vis.y1 || page.y + sy + slotH < vis.y0) continue;

          // exact key at the current step
          const exact = `p${page.index}z${d.zoomStep}r${layout.rotation}x${Math.round(sx / zoomScale)}y${Math.round(sy / zoomScale)}`;
          if (this.comp!.hasTile(exact)) {
            out.push({
              page: page.index,
              x: sx / zoomScale,
              y: sy / zoomScale,
              w: Math.min(slotW / zoomScale, page.w - sx / zoomScale),
              h: Math.min(slotH / zoomScale, page.h - sy / zoomScale),
              key: exact,
              zoomScale,
            });
            continue;
          }
          // fallback: nearest lower step tile covering this slot
          let drawn = false;
          for (let stepBack = 1; stepBack <= 6 && !drawn; stepBack++) {
            const step = d.zoomStep - stepBack;
            if (step < 0) break;
            const stepZoom = zoomForStep(step);
            const downscale = zoomForStep(d.zoomStep) / stepZoom;
            const txLow = Math.floor((sx / zoomScale) / (TILE_PX * downscale));
            const tyLow = Math.floor((sy / zoomScale) / (TILE_PX * downscale));
            const key = `p${page.index}z${step}r${layout.rotation}x${txLow}y${tyLow}`;
            if (this.comp!.hasTile(key)) {
              const scale = zoomScale * downscale;
              const srcW = TILE_PX;
              const srcH = TILE_PX;
              const tilePageW = layout.pages[page.index]!.w / zoomScale;
              const clampedW = Math.min(srcW, tilePageW - txLow * srcW);
              const clampedH = Math.min(srcH, (page.h / zoomScale) - tyLow * srcH);
              out.push({
                page: page.index,
                x: txLow * srcW,
                y: tyLow * srcH,
                w: clampedW,
                h: clampedH,
                key,
                zoomScale: scale,
              });
              drawn = true;
            }
          }
        }
      }
    }
    return out;
  }

  /** Asks the loader for thumbnails of pages near the current view. */
  private requestThumbsForVisible(res: { active_page: number }): void {
    const d = this.doc;
    if (!d || !d.layout) return;
    const from = Math.max(0, res.active_page - 4);
    const to = Math.min(d.layout.pages.length - 1, res.active_page + 8);
    const want: number[] = [];
    for (let p = from; p <= to; p++) {
      const k = `${p}@${this.thumbWidth}`;
      if (!this.thumbsRequested.has(k)) {
        this.thumbsRequested.add(k);
        want.push(p);
      }
    }
    if (want.length > 0) {
      void requestThumbnails(d.id, want, this.thumbWidth);
    }
  }
}

function clampZoom(z: number): number {
  return Math.min(64, Math.max(0.05, Number.isFinite(z) ? z : 1));
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MiB`;
}
