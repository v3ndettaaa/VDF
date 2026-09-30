# VDF — Master Implementation Plan (M0 → M8)

Status: **Approved architecture baseline.** This is the single plan of record.
Another coding agent executes exactly one milestone at a time, in order, and stops at
every milestone gate for explicit user approval.

The product priorities, locked stack, milestone structure, and quality rules are defined
in the requirements document (the "mega prompt"). This plan implements them. Where the
requirements leave a decision open, this plan decides it. There are no competing
architectures in this document.

---

## 1. How to execute this plan

1. Work one milestone at a time, in order: M0 → M8. Never start the next milestone
   without explicit user approval.
2. Within a milestone, follow the Git workflow (§35 of the requirements):
   implement → test → fix → build → validate artifact → write `reports/MX-*.md` →
   commit → push → verify GitHub Actions → STOP.
3. Never report planned functionality as implemented. Reports contain what actually
   exists and what actually passed.
4. Never weaken, skip, or delete tests to make CI green. If CI fails: inspect logs,
   find the root cause, fix, test locally, push, verify.
5. Architecture rules in §5 (dependency direction) and §6 (hot-path rule) are
   invariants. Any change to them requires a written amendment to this file and user
   approval.

---

## 2. Current repository state

- Empty repository: `main` branch, zero commits, no source files.
- `.zcodeignore` exists (workspace tool config; commit it as-is in M0 — it is
  editor tooling config, not product code).
- Everything in this plan is greenfield.

---

## 3. Locked decisions (summary)

| Area | Decision |
|---|---|
| Language / shell | Rust + Tauri v2, vanilla TypeScript UI, Vite as bundler only |
| PDF engine | MuPDF, the only PDF engine, wrapped by `mupdf-sys` (vendored pinned source) |
| License | **AGPL-3.0-only** for VDF (MuPDF is AGPL; this keeps distribution legal). Noted here so it is never "discovered" late. |
| UI stack | No framework. Hand-rolled observable store (~150 LOC). DOM owns chrome; a single WebGL2 canvas owns the viewport. |
| Viewport hot path | Rust scheduler → tile workers → binary tile transport → WebGL2 compositor. No DOM, no PNG, no JSON, no per-sample IPC in the hot path. |
| M1 tile transport | Raw RGBA over a Tauri custom URI-scheme protocol (`vdf-tile://`), behind the `RenderTransport` trait. |
| Input transport | `pointerrawupdate` + coalesced events, batched Float32Array, fire-and-forget `invoke`; stroke geometry pulled by the compositor once per frame as binary. Behind `InputTransport` trait. |
| Async runtime | None in core crates. `std::thread` + `crossbeam-channel`. Tokio exists only inside Tauri's IPC glue in `vdf-app`. |
| Edit model | All edits are commands. Persistent IDs, never array indices. Undo uses inverse data, never snapshots. |
| Persistence | Original file is never modified by opening. Autosave = append-only command log + checkpoints in an app-data workspace. Explicit Save = atomic write via MuPDF (incremental by default, full rewrite on demand). |
| OCR | `OcrEngine` adapter trait; Tesseract-backed; produces a real invisible text layer. Feature-gated, never faked. |
| Text shaping | MuPDF/HarfBuzz is authoritative for final rendering; the DOM overlay editor provides interaction (browser handles bidi/RTL correctly during editing). |
| CI | Every push: fmt, clippy `-D warnings`, tests, typecheck, builds on Windows/Ubuntu/Arch-container, artifacts uploaded. Releases only on `v*` tags. |
| Test commands | `test`, `test:unit`, `test:integration`, `test:render`, `test:e2e`, `test:performance`, `test:fuzz`, `typecheck` — created in M0, never renamed. |

---

## 4. Repository structure

```text
VDF/
├── Cargo.toml                  # workspace: crates/*, src-tauri (vdf-app)
├── rust-toolchain.toml         # pinned stable toolchain (set in M0)
├── package.json                # stable test/build command names (npm scripts)
├── tsconfig.json
├── vite.config.ts
├── crates/
│   ├── vdf-core/               # ids, errors, units, geometry, small shared types
│   ├── vdf-document/           # document/page/object model, commands, history, spatial index, serialization
│   ├── mupdf-sys/              # -sys crate: vendored MuPDF build + raw FFI (only vdf-pdf may use it)
│   ├── vdf-pdf/                # safe MuPDF integration: load, rasterize, text, annotations, save
│   ├── vdf-render/             # scheduler, tiles, zones, caches, generations, transport abstraction
│   ├── vdf-input/              # input abstraction, gesture engine, stroke pipeline, brushes, eraser
│   ├── vdf-persist/            # workspace, command log, checkpoints, atomic save, recovery, export
│   ├── vdf-search/             # text index, search/replace, OCR adapter, recognizer architecture
│   ├── vdf-diag/               # tracing, metrics, crash diagnostics, system info
│   └── vdf-bench/              # scenario benchmarks, baseline gates (PASS/WARN/FAIL)
├── src-tauri/                  # crate `vdf-app`: Tauri shell, IPC, protocol handlers, platform code
├── ui/                         # vanilla TS frontend (see §9)
├── thirdparty/
│   └── mupdf/                  # git submodule, pinned exact release tag (added in M1)
├── tests/                      # cross-crate integration tests + fixtures + golden baselines
│   ├── fixtures/               # sample PDFs (small, generated, malicious corner cases)
│   ├── golden/                 # golden render images + pixelmatch-style tolerance config
│   └── vectors/                # generated transform test vectors shared with TS tests
├── fuzz/                       # cargo-fuzz targets
├── packaging/
│   └── arch/PKGBUILD           # native Arch package (makepkg → .pkg.tar.zst)
├── reports/                    # milestone reports M0..M8
└── docs/                       # ADRs (architecture decision records) for anything that deviates
```

---

## 5. Crate responsibilities and dependency graph

Dependency direction (enforced by review; violations are build errors by design since
workspace only allows the edges below):

```text
              vdf-core
       ┌───────┼──────────┬─────────────┐
  vdf-document  vdf-diag  vdf-input     vdf-render ──── vdf-pdf ──── mupdf-sys
       │                  │   │            │
  vdf-persist ────────────┘   │            │
       │                      │            │
  vdf-search ─────────────────┴────────────┘
       │
    vdf-bench (may depend on all core crates)
       │
    vdf-app (depends on all)
```

Hard rules:

- Only `vdf-pdf` depends on `mupdf-sys`. No exceptions.
- `vdf-core` depends on nothing but `std` (+ tiny vetted deps: `serde` for schema types is
  allowed in `vdf-document`, not `vdf-core`).
- `vdf-render`, `vdf-input`, `vdf-persist`, `vdf-search` never depend on Tauri, DOM,
  browser APIs, or any IPC mechanism. They expose traits; `vdf-app` implements/adapts.
- `vdf-render` depends on `vdf-pdf` (higher level wraps lower level). Rasterization is
  still behind a `Rasterizer` trait (impl: `MupdfRasterizer` in `vdf-render`; test fake:
  synthetic rasterizer) so tile logic is testable without MuPDF.
- `vdf-input` depends only on `vdf-core`. It emits stroke/tool events through an
  `InputSink` trait; mapping events → document commands happens in the app/service layer,
  keeping the input engine pure and deterministic.

### Responsibility details

**vdf-core** — `Id` (u64, per-document monotonic + document instance UUID), `ObjectRef`,
typed error enum (`VdfError` with kind + source chaining), units (PDF points, device px,
radians), `Affine2`/`Rect`/`Point`/`Vec2` in f64, `TileKey`, `RenderGeneration`, small
newtypes (ZoomFactor, PageIndex, Rotation). No traits with side effects, no I/O.

**vdf-document** — document/page/object model (§7), command system + undo/redo history,
per-page R-tree spatial index (via `rstar`), serialization schema with version +
migrations. This crate contains zero PDF parsing; PDF content is referenced, not owned,
by the model.

**vdf-pdf** — the only MuPDF consumer: open/validate documents, page metadata + tree
access, rasterize page regions to RGBA buffers, structured text extraction, annotation
read/write, form field access, incremental/full save, redaction, OCR text-layer writing.
Exposes safe, `Send`-friendly handles built on the threading model in §8.

**vdf-render** — `RenderScheduler`, tile grid math, viewport zones (Visible/Near/Far),
priority scoring, tile + thumbnail caches with memory budgets, generation bookkeeping and
stale-result rejection, `RenderTransport` abstraction, zoom quantization policy. Knows
nothing about Tauri/PNG/DOM/Canvas.

**vdf-input** — `PointerSample` (x, y, pressure, tiltX/Y, azimuth, altitude, timestamp,
pointerId, deviceId, button state, tool type, hover/contact, capabilities), gesture
engine (tap, drag, pan, pinch incl. three-finger document-only pinch), stroke pipeline
(sampling → filtering → prediction → smoothing → stabilization → brush tessellation),
brush presets, eraser (stroke/partial/object/area), basic selection gestures, device
profiles with graceful degradation.

**vdf-persist** — workspace manager (one workspace dir per open document), append-only
checksummed command log, periodic checkpoints (binary object-model snapshots + log
compaction), `SaveManager` (IncrementalWriter, FullWriter, Validator, AtomicReplacer),
`RecoveryScanner` (detect → Restore/Discard/Open Original), `FileMonitor` (external
change + hash verification), `ExportManager`.

**vdf-search** — background per-page text indexer over extracted structured text,
normalized search (case folding, Unicode NFKC, Arabic/Persian normalization with ZWNJ/
tatweel handling) with byte-offset → rect mapping for highlights, regex/whole-word/
case-sensitive modes, search-in-annotations, replace (via the command system),
`OcrEngine` adapter trait, extensible `Recognizer` trait architecture (handwriting →
text/math/shape/diagram, search) so later phases are possible without redesign.

**vdf-diag** — `tracing`-based structured logs to rotating files in the diagnostics dir,
metrics registry (frame times, queue depths, cache hit/evict, IPC timings, input
latency percentiles), panic/crash handler capturing backtrace + system info, GPU/
renderer capability report. Never logs document content, paths beyond basename hash, or
credentials.

**vdf-bench** — headless scenario harness (§13) driving the real core crates without a
GUI, plus regression gates against `bench/baselines.json`.

**vdf-app** — Tauri v2 shell: window management, native dialogs, clipboard, drag/drop,
custom URI-scheme protocol handlers (`vdf-tile://`, `vdf-thumb://`), all JSON commands
(`vdf:open`, `vdf:metadata`, `vdf:outline`, `vdf:save`, …), binary input command,
stroke-pull command, per-document actor threads, platform abstraction (HiDPI/scale
events, Wayland/X11 quirks, tablet device source fallback), app lifecycle + recovery
prompt flow.

---

## 6. Hot-path rule (enforced architecture)

**Rendering path:**

```text
MuPDF page raster (tile workers)
  → generation-tagged RGBA tile
  → RenderTransport (M1: custom-URI binary response)
  → WebGL2 compositor (single canvas owns viewport)
  → screen
```

**Input path:**

```text
pointer events (pointerrawupdate + getCoalescedEvents)
  → batched Float32Array (per animation frame)
  → invoke('vdf:input_batch')          [fire-and-forget, no per-sample await]
  → vdf-input gesture/stroke engine    [synchronous, cheap, never blocks on render]
  → compositor pulls stroke geometry 1×/frame (binary)
  → screen
```

TypeScript/DOM owns: toolbar, tabs, menus, sidebar, panels, dialogs, command palette,
settings, status bar, properties. It never handles: individual samples, stroke
tessellation, tile rendering, rasterization, compositor frames, per-frame document
rendering, high-frequency gesture math.

**Display-only mirrors.** Two places intentionally keep a client-side approximation that
is authoritative nowhere: (a) predicted input points, (b) immediate zoom/pan transforms
applied to existing tiles during a gesture. Both are corrected by Rust-confirmed state
every frame and never become document state.

---

## 7. Document / object model

```text
VdfDocument
├── metadata (schema version, created/modified, source file hash + path)
├── pages: Vec<PageId> + PageId → PageModel
│     PageModel { pdf_ref, size (pt), rotation, label, objects: RTree<ObjectId> }
├── objects: ObjectId → ObjectModel
├── settings (default units, view prefs)
├── view state (per-tab: page, zoom, scroll, mode) — UI-owned, persisted separately
├── history: CommandLog (bounded ring of applied commands)
└── recovery: workspace handle (§11)
```

Object model common fields: persistent `Id`, `ObjectType`, `PageId`, `Affine2`
transform, cached `Rect` bounds, z-order (u32 sequence, not array position), visibility,
lock, opacity, name/metadata map.

Object types (M2 adds Ink; M3 adds the rest; fields given in full here so the schema is
designed once):

- `Ink` — points with `{x, y, pressure, tilt_x, tilt_y, azimuth, altitude, t_ms}` in
  f32 sub-fields, packed; brush id + resolved brush params snapshot; bbox.
- `Shape` — kind (line, arrow, double-arrow, rect, rounded-rect, circle, ellipse,
  triangle, polygon, star, pentagon, hexagon, cloud, callout, arc, bezier, polyline,
  freeform), geometry params, style (fill, stroke, gradient, opacity, dash, joins,
  caps, corner radius, rotation).
- `TextBox` — styled runs (font, fallback chain, size, weight, italic, underline,
  strikeout, color, highlight, alignment, line spacing, lists/bullets/numbering, super/
  subscript, bidi paragraph direction), content as versioned rich-run tree; rendered by
  MuPDF FreeText on save, live-edited in DOM overlay.
- `Image` — referenced decoded-size + original byte reference (kept in the workspace
  blob store; untouched bytes are reused, never re-encoded), crop/mask/transform.
- `Highlight` — quad list + color/opacity, text-range backref when source is a text
  selection.
- `Note` (sticky note), `Stamp`, `RedactionMark` (until promoted to true redaction).
- `PdfAnnotationRef` — mirrors a real PDF annotation (updated in place on save).
- `Group` — ordered child ids.

Layers are per-page ordered lists of layer descriptors (PDF Content, Handwriting,
Highlights, Shapes, Images, Text, Notes + user layers) with hide/show/lock/opacity/
isolate; objects carry a layer id.

**IDs.** `ObjectId(u64)` monotonic per document epoch; never reused (deletion creates a
tombstone so undo can restore). Serialized as `"o17"`-style strings in JSON contexts.
Array indices are never identity anywhere.

**Coordinates.** Canonical document space = PDF user space, f64, points, origin at each
page's lower-left, y-up (rotation applied per page /Rotate). Chain:

```text
PDF space (pt, y-up)  --page transform-->  document space (pt, y-up, page-normalized)
  --view transform (zoom, rotation, layout)-->  viewport space (device px, y-down)
  --devicePixelRatio-->  screen space (CSS px)
```

All conversions live in Rust (`vdf-core` math + `vdf-render` view transform). The TS
compositor applies transforms it receives; the TS mirror of `Affine2` is minimal and
verified against Rust-generated golden vectors (`tests/vectors/affine-cases.json`,
generated by a Rust test, consumed by vitest) so the two never drift silently.

**Serialization.** Schema version + migration functions `v(N) → v(N+1)`; unknown fields
preserved; snapshots are binary (postcard/serde) with a JSON debug mode; corruption is
detected via checksums per record.

---

## 8. Command system, threading, and concurrency

### Commands

Every mutation is a `Command` with `execute`, `undo`, `redo`, and a serialized form
(the same form the recovery log stores). Commands carry inverse payloads (erase stores
the removed segments; move stores the delta) — undo never snapshots the document. One
stroke = one command. Partial erase splits strokes into reversible segment edits.
Command application is single-threaded (document actor) and deterministic; this is what
makes replay-based recovery and property tests possible.

### Threading model (no async runtime in core)

| Thread | Owns | Role |
|---|---|---|
| Main | Tauri event loop | window/UI events, IPC entry |
| Document actor (per open document) | MuPDF master `fz_context`, document handle, object model, history | serial command application, load/save/structure ops |
| Render workers (N = min(cores−1, 6)) | each a `fz_clone_context` of the master | parallel tile rasterization only (the MuPDF-supported clone-context pattern; page-tree access is guarded by MuPDF's locks) |
| Scheduler | priority queues, generation table, caches | decides what renders next, receives results, evicts |
| Background services | — | search indexing, OCR, autosave flush, file monitoring (each a plain `std::thread` with channels) |

Communication: `crossbeam-channel`. The input batch handler runs the (cheap) gesture +
stroke-stage work synchronously on the IPC thread and never awaits renders, OCR, saves,
or indexing. If the MuPDF clone-context pattern shows any instability under stress
(M1 stress test), the fallback is a single render worker — a config value, not a
redesign.

### Caching and memory

- `MemoryGovernor` owns budgets, configurable, defaults derived from total RAM:
  tile cache ≈ 25% RAM, thumbnails ≈ 128 MiB, MuPDF store (`fz_store` cap) ≈ 25% RAM,
  text index ≈ 256 MiB (spill = load per page on demand).
- Eviction order: Far-zone tiles → Near-zone tiles → thumbnails → index segments.
  Never evict: visible tiles, active stroke data, current document model, UI state.
- Tile cache key = `TileKey { doc_rev, page, zoom_step (quantized u16), rotation, x, y }`
  plus a monotonic `render_generation`. A result is accepted only if its generation ≥
  the generation currently recorded for that slot; late stale results are dropped and
  counted in diagnostics. This is the mechanism behind the no-overwrite invariant (§10).
- Zoom quantization: continuous zoom for display; tile requests snap to steps of
  2^(1/8) (≈ +9%), so stale tiles are never upscaled more than ~9% before replacement.

---

## 9. Frontend architecture (`ui/`)

```text
ui/
├── app/            # bootstrap, lifecycle, wiring to vdf-app IPC
├── viewport/       # compositor (WebGL2 canvas), tile fetcher, ink layer, zoom mirror
├── input/          # pointer capture, batching, preview stamps (display-only)
├── components/     # buttons, menus, tooltips (small, hand-written)
├── toolbar/        # contextual toolbar + tool shelf
├── sidebar/        # pages, outline, search, layers, properties panels
├── tabs/           # document tabs, split views, reopen-closed
├── panels/ dialogs/ command-palette/
├── state/          # observable store (~150 LOC), typed slices, event bus
├── shortcuts/      # keymap, customizable shortcuts, discovery
├── accessibility/  # focus management, ARIA labels, reduced motion, scaling
└── styles/         # CSS custom properties, themes (Dark AMOLED #000000, Light, System)
```

- Each module owns its DOM subtree and talks to others only through the store, the
  event bus, or explicit interfaces. No giant scripts.
- Vite is bundler/dev server only. TypeScript strict mode; ES modules; no framework,
  no state-management library, ever.
- The compositor is behind a small TS-side `RendererBackend` interface (WebGL2 now;
  allows a WebGPU/native option in M7 without touching callers).
- UI dark mode and document dark-view are independent: dark-view is a compositor
  shader mode (luminance inversion preserving hue), never a CSS filter over the DOM.

---

## 10. Rendering architecture

### No-blanking / stale-render invariant

Never clear a viewport region because new content is pending. During zoom, scroll,
rotation, page transitions, and hi-res upgrades, existing tiles stay on screen —
transformed and/or scaled — until generation-matched replacements arrive. Black frames,
white frames, and forced loading screens in the viewport are defects with regression
tests:

- stale-render test: render A@100%, render B@200%, force B to land first → late A must
  not overwrite B;
- zoom-continuity test: during the zoom ladder, at no frame is the visible region
  backed by zero tiles.

### Scheduler

- Zones: Visible (intersect viewport), Near (±2 pages, prefetch at ¼ resolution),
  Far (thumbnails only).
- Priority score: zone, distance to visible rect, scroll-direction alignment (prefetch
  ahead of motion), zoom activity (suppress far hi-res during gestures), interaction
  (once ink exists: active-stroke pages outrank everything), fast-jump detection
  (scroll velocity > threshold → drop intermediate pages to low-res previews, render
  full quality after settle).
- Tiles: 256×256 device px initial target; emitted in spiral order from the viewport
  center. This number is a hypothesis to validate in M1/M7 profiling, not a constant to
  defend.

### Transport (abstraction first, practical M1, optimized M7)

`vdf-render` defines:

```rust
trait RenderTransport {
    /// Publish a finished, generation-tagged tile; must not block the scheduler.
    fn publish_tile(&self, tile: FinishedTile) -> anyhow::Result<()>;
    /// Invalidate transport-side resources for a document/revision.
    fn invalidate(&self, doc: DocHandle, rev: Revision) -> anyhow::Result<()>;
}
```

M0: trait + an in-memory fake (used by tests).
M1: `vdf-app` implements it over a Tauri custom URI-scheme protocol: the compositor
fetches `vdf-tile://<doc>/<tilekey>?gen=<n>` and receives a binary response
(16-byte header: version, flags, generation, payload length — then raw RGBA). No PNG,
no base64, no JSON, no per-tile Tauri events. Throughput and latency are measured and
recorded in the M1 report.
M7: profile under heavy workloads; candidate optimizations (response streaming batches,
shared-memory staging, reduced copies, compositor backend swap) all live behind this
trait — the document renderer never learns the mechanism.

---

## 11. Input architecture

- Capture in the webview: `pointerdown/move/up` with `pointerrawupdate` +
  `getCoalescedEvents()` for full-rate samples (pressure, tiltX/Y, twist, pointerId,
  tool type, buttons — barrel button and eraser end included where the platform
  delivers them). `touch-action: none`; browser/page zoom gestures disabled at the
  Tauri webview level so Ctrl+wheel and pinches belong to VDF.
- Batching: accumulate per animation frame into a `Float32Array`
  (`[ptrId, toolFlags, x, y, pressure, tiltX, tiltY, tMs] × N`), send with one
  fire-and-forget `invoke('vdf:input_batch', buffer)`. No per-sample await/ack, no
  per-sample UI state updates.
- Gesture engine (Rust): tap/double-tap, drag, two-finger pan/zoom, three-finger pinch
  — which scales **document space only, never UI** — wheel zoom, keyboard zoom.
  All zoom outputs feed one `DocumentZoomController` (Rust-authoritative state;
  compositor mirrors for immediate feedback).
- Stroke pipeline: sampling → filtering (dropout/debounce) → prediction (display-only,
  never persisted, stripped before the stroke model) → smoothing → stabilization
  (leashed/ Spring-style, time-aware) → brush engine (tessellation to triangle stamps
  with spacing/taper/pressure-tilt-velocity response) → immediate renderer.
- Live feedback without duplicating the brush engine: the compositor draws confirmed
  tessellation pulled from Rust once per frame (`invoke('vdf:poll_strokes')` → binary),
  plus a thin display-only preview (round-capped polyline stamps in current color/size)
  for in-flight/predicted points only. The Rust brush engine is the single source of
  rendering truth.
- Device profiles: per-device pressure curve, tilt mapping, hover behavior; devices
  without pressure/tilt get sensible constant/velocity-derived fallbacks (tested).
- Linux/Wayland stylus fidelity risk: if WebKitGTK pointer events prove insufficient
  (rate, pressure, coalescing), `vdf-app` adds a native tablet source (libinput/evdev
  on Linux; WM_POINTER already flows through WebView2 on Windows) that feeds the same
  `InputTransport` — decided by an M2 spike, not pre-built.

---

## 12. Persistence, save, recovery, export

- Opening a PDF never writes to it. Each open document gets a workspace:
  `~/.local/share/vdf/workspaces/<doc-hash>/` (platform-appropriate base) containing
  `manifest.json` (source path, content hash, schema version, revision), `commands.log`
  (append-only, checksummed command records), `checkpoints/` (binary snapshots; writing
  one compacts the log), `blobs/` (inserted images, original bytes).
- Autosave appends to the command log — it never rewrites the PDF. Checkpoints on a
  timer (e.g., every 200 commands or 5 min idle) and on clean close.
- Explicit Save (atomic): render edited document via MuPDF → write temp file in the
  target directory → flush → fsync → validate (re-open + parse + page count + spot
  render) → atomic rename over target. Interruption at any point leaves the original
  valid. Default writer: incremental append when FileMonitor confirms the source is
  byte-identical since open (fast, preserves original bytes); full rewrite (compact,
  deflate) for Save As / export / when incremental is impossible. Signed-document
  modification always warns first.
- Recovery: on launch, `RecoveryScanner` finds workspaces with uncheckpointed
  commands → prompt Restore / Discard / Open Original. Restore = replay command log
  over the original file (source hash must match; mismatch is reported honestly).
  Recovery data contains no passwords; encrypted documents require re-authentication.
- Detection on open: encrypted (offer password unlock; MuPDF crypto), signed (warn on
  edit), linearized, object streams, malformed (MuPDF error surfaced typed), forms
  (AcroForm detection), permission flags.
- Export is separate from Save: flatten, dark PDF, optimize/reduce-size, page-range,
  image-only, print-ready — produced by ExportManager from the document model.
- True redaction (M5): MuPDF `pdf_redact_page` to actually remove content; verification
  tests must prove the redacted strings are absent from extracted text, search, and the
  decompressed content streams of the output file.

---

## 13. Search / OCR / benchmarks / diagnostics

- Search: background indexer builds per-page normalized text with offset→rect maps;
  supports case-sensitive, whole-word, regex (`regex` crate, Unicode), all-pages,
  annotations, highlight-all, next/prev, replace (command-based). Tested on English,
  Persian, Arabic, RTL/LTR, mixed-script, numbers, symbols.
- OCR: `OcrEngine` trait; Tesseract-backed engine (integration route — MuPDF's optional
  Tesseract vs. direct `libtesseract` binding — chosen by an M4 spike; feature-gated
  because it is a system dependency). Output = invisible text layer written through
  MuPDF, preserving original appearance, with confidence values. OCR always runs on
  background services; it can never block input.
- Handwriting intelligence: `Recognizer` trait (`InkCanvas → RecognizedOutput`) with
  text/math/shape/diagram result kinds and an ink-query index architecture. M4
  delivers the architecture and maybe shape recognition; nothing is faked.
- Benchmarks (`vdf-bench`, headless, real crates): (1) startup with 100-page PDF,
  (2) open 1000-page PDF, (3) fast scroll 1→100→500→900, (4) zoom ladder
  100→150→200→300→500→100, (5) ink while rendering. Metrics: startup time, first
  useful page, peak RSS, queue depth, dropped frames (from diag counters), input
  latency percentiles, transport overhead. Output PASS/WARN/FAIL vs
  `bench/baselines.json`; first runs establish baselines without gates; FAIL =
  freeze/crash/intentional blanking/stale overwrite/unbounded memory/input starvation.
- Diagnostics: `vdf-diag` metrics registry + rotating structured logs + crash handler;
  a diagnostics panel (M6) surfaces renderer backend, queue/cache stats, and input
  latency. No document content, no credentials, hashed filenames only.

---

## 14. Testing strategy and stable commands

Pyramid (targets live in the owning crates; cross-crate scenarios in `/tests`):

- **Unit**: document model, IDs, geometry, transforms, coordinate chains, commands,
  undo/redo, serialization/migrations, selection, search normalization, redaction
  logic, stroke pipeline stages, cache/generation logic.
- **Property-based** (`proptest`): transform round-trips, command
  execute→undo→redo equivalence, serialization ⇄ deserialization equivalence,
  invariant checks (no ID reuse, no orphaned objects).
- **Rendering (golden)**: text, transparency, images, vectors, annotations, RTL,
  rotation, clipping, zoom levels; full-page vs tiled equivalence; tile boundaries and
  seams; missing-tile placeholders (never blank — placeholder = parent low-res tile);
  stale generations; zoom continuity. Tolerance-based image diff with an explicit
  `regen` command for intentional changes.
- **Input**: pointer/device matrix (mouse, touch, stylus, no-pressure devices),
  pressure fallback, prediction exclusion from persistence, smoothing invariants,
  eraser reversibility, undo/redo, batch ordering.
- **Persistence/recovery**: interrupted save (kill points at each stage), atomic
  replacement failure injection, recovery replay, corrupted/malformed/encrypted/signed
  PDFs, workspace corruption.
- **Security**: redaction non-recoverability proofs.
- **Fuzz** (`cargo-fuzz`): PDF open/parse via the safe layer, command-log and manifest
  deserialization, object-model mutation. CI runs short smoke runs per push.
- **E2E**: primary = Rust headless scenario tests over the real crates
  (open→draw→erase→text→image→save→close→reopen; search→navigate→edit→undo→redo→
  export). Secondary = tauri-driver GUI smoke on Linux CI (Xvfb) once per push.

Stable commands (npm scripts at root, delegating to cargo/vitest internally; created in
M0, never renamed): `test`, `test:unit`, `test:integration`, `test:render`,
`test:e2e`, `test:performance`, `test:fuzz`, `typecheck`. TypeScript tests complement,
never replace, Rust behavior tests.

---

## 15. CI and packaging

**GitHub Actions, every push:**

- `lint-test` (ubuntu): `cargo fmt --check`, `cargo clippy --workspace --all-targets
  -- -D warnings`, `cargo test --workspace`, `tsc --noEmit`, `vitest run`.
- `build-linux` (ubuntu-24.04): `tauri build` → `.deb` + AppImage; upload artifacts.
- `build-windows` (windows-latest): `tauri build` → `.msi` + portable zip; upload.
- `arch` (container `archlinux:base-devel`): install toolchain, `makepkg -f` from
  `packaging/arch/PKGBUILD` → `.pkg.tar.zst`; upload. AppImage is explicitly **not**
  the Arch deliverable.
- Caches: `Swatinem/rust-cache` per job, MuPDF submodule + build cache, pacman cache
  for Arch.
- Release job: runs only on `v*` tags; attaches all artifacts; draft release notes.
  Signing architecture (Windows signtool via secrets, Linux checksums) is prepared in
  M8 plumbing; no keys in the repo.

**MuPDF build strategy (M1):** vendored source pinned by submodule commit;
`mupdf-sys` builds it via its own make in release/static config with pthread locking
enabled; Windows uses an LLVM clang-cl toolchain through the same make (MSYS2 make on
the CI image). This is the top risk — the M1 spike validates it first, before any
rendering work. If neither make route works on MSVC, the fallback is a CI job that
builds static archives and a `prebuilt` feature in `mupdf-sys` that fetches them by
checksum — same FFI surface either way, so no architectural change.

---

## 16. Milestones

Each milestone ends: tests → build → validate artifact → `reports/MX-*.md` (per the
required report template) → commit → push → verify CI → **STOP for user approval**.

### M0 — Foundation (no MuPDF)

**Build:** Cargo workspace with all crates as skeletons (real `vdf-core`, working
geometry/ids/errors; trait-only stubs elsewhere: `RenderTransport` + in-memory fake,
`Rasterizer` fake, `InputSink` no-op, persistence API types); Tauri v2 app with window,
theme switching (Dark AMOLED/Light/System), UI shell (tabs bar, toolbar placeholder,
sidebar placeholders, status/zoom bar, working command palette over a static command
list), observable store, `ui/` module layout, keyboard shortcut skeleton; stable test
commands + `typecheck`; CI (all four jobs above with crates stubs; arch PKGBUILD
prototype building the stub app); packaging config; rust-toolchain pinned; `.gitignore`
+ `.zcodeignore` committed; ADR dir.
**File → Open:** native dialog → filename + size displayed in UI. No parsing, no
rendering — enforced by the fact that `mupdf-sys` does not exist yet.
**Tests:** vdf-core unit + property tests (geometry round-trips, id monotonicity),
store/event-bus vitest, TS-vs-Rust affine golden vectors, CI green on all three OSes,
artifacts downloadable.
**Manual validation:** app launches on Linux (and Windows if available), UI visible,
themes switch, command palette filters/executes, File → Open shows name + size, CI
artifacts exist.
**Explicitly out of scope:** any PDF parsing/rendering, real transports, input engine.

### M1 — PDF Viewer

MuPDF integration (`mupdf-sys` vendored + `vdf-pdf`: open, validate, page tree,
metadata, rasterize region, stext extraction ready); progressive loading (page
lifecycle states, load-on-demand); `vdf-render` scheduler + tile renderer + tile/
thumbnail caches + generation system; custom-URI binary transport + WebGL2 compositor;
scrolling (continuous mode first), page modes (single/continuous/two-page/book),
navigation (outline, bookmarks, go-to-page, back/forward history); `DocumentZoomController`
with focal-point zoom, Ctrl+wheel, pinch, three-finger document-only pinch; HiDPI +
fractional scaling (scale-change re-tiling without blanking); malformed/encrypted/
signed detection surfaces typed errors; benchmark baseline run recorded.
**Tests:** golden rendering set, tiled-vs-full equivalence, seam tests, stale-render
(100%/200% ordering), zoom continuity, malformed-PDF corpus, 1000-page fixture
behavior (memory bounded, no per-page full parse).
**Manual:** 1000+ page PDF opens usable quickly, smooth scroll, immediate zoom, no
blanking, no stale overwrite, three-finger pinch scales document only.

### M2 — Handwriting

`vdf-input` in full: pointer/stylus abstraction, batched input transport, gesture
engine, stroke pipeline (filtering, prediction display-only, smoothing,
stabilization), brush engine + presets (Pen, Pencil, Fountain, Ballpoint, Marker,
Highlighter, Brush, Calligraphy, Technical Pen, Custom, Laser), pressure/tilt, device
profiles + degradation, eraser (stroke + partial split with reversibility), basic
selection; ink objects integrated with document model + undo (one stroke = one
command); Linux stylus fidelity spike (decide native tablet source or not).
**Tests:** device matrix, pressure fallback, prediction-never-persisted, smoothing
invariants, eraser + undo round-trips, input latency percentiles recorded.
**Manual:** tablet pressure, tilt, hover, barrel button, eraser end, writing while
background rendering is active.

### M3 — Editing

Complete selection (lasso/rect/object/stroke/area + move/resize/rotate/duplicate/
copy/cut/delete/recolor/group/ungroup/lock/z-order/align/distribute); geometry tools
(ruler, protractor, compass, grid, snapping, smart guides, measurements, calibration);
all shapes with properties; shape recognition (threshold-gated, undoable); text boxes
+ rich text + RTL/LTR + Persian/Arabic + mixed scripts (DOM overlay editor → MuPDF
FreeText; overlay-based editing of existing PDF text where direct edit is unsafe —
never corrupt the original); images (insert/paste/drag-drop/resize/crop/rotate/flip/
opacity/mask/border/shadow/replace/extract, no re-encoding of untouched bytes);
PDF annotations (highlight, underline, squiggly, strikeout, ink, freetext, sticky
note, stamp, caret, file attachment, link, polygon, polyline, circle, square, line,
popup — redaction exists as a mark object only); page management (add/delete/
duplicate/move/reorder/rotate/crop/resize/extract/replace/insert/merge/split/clone/
import/export, labels, blank pages, custom sizes); layers.
Every edit goes through the command system — enforced by review + a test that
mutating APIs are command-based.

### M4 — Search & Intelligence

Background text index; search UI (case/whole-word/regex/all-pages/annotations/
highlight-all/next/prev); replace via commands; RTL/Persian/Arabic test corpus; OCR
adapter + Tesseract integration (spike-chosen route), language selection, confidence,
page/document OCR, invisible text layer, background-only execution; `Recognizer`
architecture for handwriting intelligence (implement shape recognition if it earns its
place; nothing faked).

### M5 — Persistence & Security

Save/Save As/Export; IncrementalWriter + FullWriter + Validator + AtomicReplacer;
autosave; command log + checkpoints + compaction; FileMonitor; recovery flow
(Restore/Discard/Open Original) with workspace-scanner UI; encryption unlock +
permissions; signature awareness + warnings; forms reading/filling; **true redaction**
with verification tests proving non-recoverability (text extraction, search, content
streams). Save-safety tests incl. interrupted-save kill points.

### M6 — Product UI

Toolbar/menus/tabs/sidebar/properties polish; notebook mode; themes finalized;
distraction-free + presentation modes; split views; reopen-closed tabs; customizable
shortcuts + discovery; contextual menus; floating selection controls; accessibility
pass (keyboard nav, focus management, high contrast, scalable UI, reduced motion,
screen-reader labels, color-independent states); tauri-driver E2E + accessibility
validation added to CI.

### M7 — Performance & Quality

Profile real workloads; optimize rendering, scheduler, transport (behind
`RenderTransport` — including zero-copy/low-copy candidates), input, memory, caches,
loading. Hardware matrix: low-end iGPU, mainstream, stronger desktop/laptop (the
AMD Lucienne / 14 GiB machine is one profile, not the definition). Run fuzzing
campaign, benchmark gates vs baselines, large-document soak, memory testing, software
rendering fallback, crash/recovery drills. Update baselines with justification.

### M8 — Packaging & Release

Windows MSI + portable; Linux DEB + AppImage; Arch PKGBUILD → `.pkg.tar.zst`;
validate install → launch → open → annotate → save → reopen on each target (CI
automated where possible, manual where not — reported honestly); signing architecture
prepared. Tag `v0.1.0` **only after explicit user approval**.

### Dependency graph

Strictly sequential M0→…→M8. Notable internal prerequisites: M1's transport feeds M2's
compositor ink layer; M3's commands feed M5's log/recovery; M4's OCR reuses M1's
extraction; M7 profiles everything; M8 packages M7's output.

---

## 17. Risks

| # | Risk | Mitigation |
|---|---|---|
| 1 | MuPDF build on Windows/MSVC | M1 week-1 spike; clang-cl via make; prebuilt-archive fallback behind identical FFI |
| 2 | MuPDF thread-safety misuse | clone-context pattern only; stress test; single-worker config fallback |
| 3 | AGPL obligations | VDF is AGPL-3.0-only; decision recorded here; commercial route via Artifex if ever needed |
| 4 | WebKitGTK stylus fidelity (Linux) | M2 spike; native libinput/evdev source behind `InputTransport` if needed |
| 5 | Custom-protocol throughput | measured in M1; abstraction allows M7 replacement/batching/shared-memory |
| 6 | M3 scope explosion | command-system-first; features land only with tests + report honesty |
| 7 | Recovery log growth | checkpoints + compaction; bounded history |
| 8 | Fractional scaling / multi-monitor | scale-change re-tiling keeps old tiles visible; manual matrix in M1 + M7 |
| 9 | OCR as heavy system dep | feature-gated adapter; never blocks input; no fake functionality |
| 10 | tauri-driver flakiness | primary E2E is Rust headless scenarios; GUI smoke is secondary |

---

## 18. Quality rules (binding)

Never: disable/delete/weaken tests; fake unsupported functionality; claim untested
platforms or hardware; couple Rust core to DOM/Tauri/browser details; add frameworks
or a second PDF engine; make benchmark-driven correctness trade-offs.

Prefer: simple abstractions, measured performance, clear ownership, explicit
concurrency, deterministic tests, strong invariants, real profiling, honest
limitations.

Success criterion, in order: open a large PDF → useful quickly → smooth scrolling →
instant zoom → natural handwriting → reliable editing → safe save → safe recovery →
successful reopen.
