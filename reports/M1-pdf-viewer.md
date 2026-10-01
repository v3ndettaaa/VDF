# M1 — PDF Viewer: Milestone Report

Date: 2026-10-01
Branch: `main`
Milestone commit: `0610d18cf83db53e8e8faf73844892b5a143f5e8` (hash recorded in the bookkeeping commit that follows — a commit cannot contain its own hash)
CI: _verified after push (results at bottom)_

---

## Objective

Integrate MuPDF and deliver the working viewer per MASTER_PLAN.md §16/M1: progressive
loading, tile renderer + scheduler + caches + generations, binary rendering transport,
WebGL2 compositor, scrolling, page modes, focal-point zoom (incl. pinch), navigation,
HiDPI, malformed/encrypted detection, and the benchmark baseline — with the stale-render
and no-blanking invariants enforced and tested.

## Completed work

**mupdf-sys (new crate)** — the vendored engine boundary

- MuPDF **1.28.5** pinned as a git submodule (`thirdparty/mupdf`, recursive), built from
  source by `build.rs` (`make build=release XCFLAGS=-fPIC` → `libmupdf.a` +
  `libmupdf-third.a`, ~57 MiB static, linked into every artifact).
- Raw FFI declarations verified against the pinned headers (`fz_matrix`/`fz_rect`
  layouts, `fz_new_context_imp` — `fz_new_context` is a C macro, the archive exports
  only the `_imp` symbol; version string passed via build-time env read from
  `version.h` so the FFI and the pinned source can never drift).
- **C shim (`shim.c`) for every fallible call**: MuPDF reports errors with
  setjmp/longjmp; a longjmp across FFI is UB, so every `fz_*` call that can throw is
  wrapped in a C function that contains `fz_try/fz_catch` in C and returns 0/1 plus an
  error-message buffer. No longjmp ever crosses into Rust.
- `FzContext` with spinlock-backed `fz_locks_context` (FZ_LOCKS=3: ALLOC/FREETYPE/
  GLYPHCACHE) and clone support; document handlers registered explicitly.
- Smoke tests: open/count/bound, pixel-verified render (blue rect at exact PDF-mapped
  coordinates), malformed input errors instead of panicking, cloned-context rendering.

**vdf-pdf** — the safe engine API

- `MupdfEngine` (one master context) → `PdfDocument` (open from bytes; **the original
  file is never written**). Typed errors: `Malformed`, `Encrypted` (via
  `fz_needs_password`), `Engine`, `Io`.
- Threading follows MuPDF's official rules (docs/reference/c/overview.md §Multi-threading):
  **rule 2 forbids simultaneous document access** — so the document actor builds
  per-page **display lists** (`fz_new_display_list_from_page`) and workers run them
  concurrently (`fz_run_display_list`). A behavior test renders 4 display lists from 4
  threads with a barrier.
- Correct tile math: closed-form ctm for `rot` quarter turns **clockwise** in y-down
  page space with zoom folded in, verified by pixel-probe tests for D0/D90/D180/D270
  (an asymmetric page mark must land in the mapped corner of the rotated view).
- Tiled-vs-full equivalence test (stitched 256-px tiles vs one region render: interior
  diff ≤ 3, boundary AA diff ≤ 16).
- Malformed corpus (empty/garbage/truncated-xref/bad-magic/random bytes) → typed
  errors or safe repair, never a panic. 1000-page doc: `page_count` < 1 ms (page tree,
  no sweep), distant page-999 display list + render ~8 ms.
- Outline tree (`fz_load_outline`), password API, out-of-range errors clean.

**vdf-render** — the rendering pipeline

- `layout.rs`: continuous / single / two-page layouts in view space (device px), with
  page-rect queries, hit-testing, rotation-aware sizing. Unit tests for stacking,
  centering, two-page rows, rotation swaps, zoom scaling.
- `zoom.rs`: `DocumentZoomController` — clamped 5%–6400%, **focal-point zoom**
  (document point under the cursor/pinch stays put, verified by test), scroll state.
- `cache.rs`: LRU `TileCache` with byte budget, generation guard at the boundary
  (stale results rejected — defense in depth with the transport), pinning (visible
  tiles never evicted), predicate invalidation, RAM-derived `MemoryBudgets`.
- `scheduler.rs`: viewport snapshots → wanted-tile computation (visible + near band +
  scroll-direction prefetch) → priority queue (zone, distance, ahead-bonus) → bounded
  job channel → N worker threads (cloned contexts created lazily per thread) →
  results drained with generation checks → cache + transport publish. Burst limiter,
  pending/slot-generation bookkeeping, stale-drop counters, `TileRenderer` trait
  (real `MupdfTileRenderer` + test `FakeTileRenderer`). 7 scheduler tests cover
  viewport→tiles flow, cache-hit no-re-render, prefetch, missing-DL skip+retry,
  two-column layouts, pinning.

**vdf-app** — the shell integration

- `AppCore` (engine + doc registry) → per-document `DocCore`: scheduler, page sizes,
  layout slot (revisioned), zoom controller, page mode, rotation, dpr, thumbnail cache.
- Per-document **loader thread** (serial document work — never blocks input): background
  page-size sweep (all sizes, chunked, layout refreshed as it goes), display-list
  builds on demand, thumbnail rendering. Owns a `Weak<DocCore>` (no ownership cycle);
  channel closes on document close, then the loader exits.
- Commands: `vdf_open_document`, `vdf_close_document`, `vdf_viewport`, `vdf_poll`,
  `vdf_pan`, `vdf_zoom`, `vdf_fit_width`, `vdf_set_page_mode`, `vdf_rotate`,
  `vdf_goto_page`, `vdf_outline`, `vdf_request_thumbnails` — thin wrappers over the
  same `DocCore` methods a headless test drives (one code path).
- **Binary transports**: `vdf-tile://d{doc}/{key}` (16-byte header + raw RGBA; no PNG,
  no base64, no JSON in the hot path) and `vdf-thumb://` (404 until the loader renders
  it). Compositor fetches only keys the poll reports ready.

**UI (`ui/`)** — framework-free viewer

- `viewport/compositor.ts`: WebGL2 renderer — one texture per tile, quad batches for
  page "paper" + tiles, GPU-side texture LRU (512), DPR-aware resize.
- `viewport/controller.ts`: the interaction loop — rAF poll of authoritative state
  (scroll/zoom/layout/ready-tiles), display-only pan deltas reconciled each frame,
  wheel (pan; **Ctrl+wheel = focal zoom**), pointer-drag pan, two-pointer touch pinch,
  **no-blanking tile fallback**: when a slot's current-step tile isn't ready, the
  nearest lower-step tile is drawn scaled into place (≤ ~9% upscale at the 2^(1/8)
  ladder) — the viewport never intentionally blanks.
- Real sidebar: **Pages panel** (virtualized thumbnail grid around the active page,
  click → navigate) and **Outline panel** (live `vdf_outline` tree, click → navigate);
  Search/Layers/Properties stay honest milestone placeholders.
- Status bar: page indicator, tile-cache/rendered/stale counters, zoom mirror.
- Commands: file.open actually opens documents now (tab + status), zoom in/out/reset/
  fit-width, rotate, page modes, close document — palette/shortcut wiring updated
  (`when` visibility for document-scoped commands).

**Benchmarks + baselines**

- `vdf-bench` gained the `m1` scenario (headless, real crates): open 100/1000 pages,
  first display list, first-viewport tile batch, distant-page tile, fast-scroll jump
  (1→100→500→900, scheduler throughput with the fake renderer), zoom-ladder re-layout.
- Baselines recorded to `bench/baselines.json` (ungated, per plan): on this machine
  (12-core/14 GiB): **open 100-page 0.22 ms · open 1000-page 0.19 ms · first display
  list 1.8 ms · first viewport (12 tiles) ≈ 21 ms · distant tile 8.0 ms · scroll-jump
  scheduling 122 ms · zoom-ladder re-layout 0.13 ms**. Gates activate in M7.

## Architecture decisions

- **Display-list rendering as the concurrency unit** — MuPDF rule 2 forbids concurrent
  document access; display lists are the documented way to render in parallel. This
  also matches the plan's page-structure cache.
- **Loader thread owns all document mutation** (sizes, display lists, thumbnails) —
  the scheduler and workers never touch the document, only display lists.
- **Tile display fallback in the compositor** implements the no-blanking invariant
  without Rust-side complexity: old/lower-step tiles stay visible until replacements
  arrive.
- **Focal zoom is Rust-authoritative**; the UI applies a display-only factor
  immediately and reconciles on the next poll (plan §10 mirrors).
- Full details in `docs/adr/0002-m1-viewer-decisions.md`.

## Files / modules created

`crates/mupdf-sys/` (build.rs, shim.c, raw.rs, lib.rs, smoke tests), `thirdparty/mupdf`
submodule @ 1.28.5, `crates/vdf-pdf/` (lib + behavior/isolation tests + fixture
generator), `crates/vdf-render/` (layout/zoom/cache/scheduler/renderer + tests),
`src-tauri/src/{state,commands,protocols}.rs` + headless e2e test, `ui/viewport/
{compositor,tiles,controller,compositor-helpers}.ts` + rewritten index, updated
sidebar/statusbar/commands/ipc/styles, `bench/baselines.json`, CI updates (submodule
checkout, Windows MinGW route, PKGBUILD submodule init), fixture writer bin.

## Tests written / executed

Executed locally (Rust 1.98.1, Node 26):

| Suite | Result |
|---|---|
| `cargo test --workspace` | **all suites pass** (mupdf-sys 4+1, vdf-pdf 9+1+1, vdf-render 17+7, vdf-app integration 7 + **headless e2e 1**, document 10+2, core 22+3+1+1, search 5) |
| `cargo fmt --check` / `clippy -D warnings` | clean |
| `npm run typecheck` | 0 errors |
| `npx vitest run` | **25/25** (incl. Rust↔TS golden vectors) |
| `vdf-bench --scenario m1` | PASS (metrics above) |
| `npm run tauri build` | .deb 31.5 MiB · .rpm 31.5 MiB · .AppImage 126.7 MiB |
| Launch | release binary runs, clean stderr |

**Headless e2e** (drives the real AppCore: open → viewport → poll → tile pixels →
thumbnails → outline → goto → pan → zoom → two-page mode): passes in ~0.02 s wall with
open 1.5 ms, first tiles 6.5 ms.

Fixes made during the loop (all found by tests/debugging, none hidden):

- Shim passed the same ctm to both the draw device and `fz_run_page` — transforms
  applied twice; identity rendered fine (masking the bug) but every crop/zoom/rotation
  was wrong. Device now gets `fz_identity`.
- First concurrency design raced document access (parse errors under concurrent
  render) — redesigned around display lists per MuPDF rule 2; the earlier "passing"
  concurrent test was vacuous because of the double-ctm bug. Both fixed together.
- `last_viewport` was read by retry but never written — after the loader built a DL,
  no tile re-request happened (found via file-based event tracing through a hang that
  ptrace could not inspect).
- Shutdown race: `update_viewport` panicked when the job channel closed mid-update —
  now treated as "scheduler stopped".
- `fz_new_context` is a macro (`fz_new_context_imp` + version string) — linker
  resolved only the `_imp` symbol; version now flows from version.h via build.rs.
- Reserved keyword `gen` (Rust 2024), float type annotations, borrow conflicts
  (pinning closure snapshots the viewport), clippy lints.

## Build result

Release build bundles (with MuPDF statically linked): `vdf_0.1.0_amd64.deb` (31.5 MiB),
`vdf-0.1.0-1.x86_64.rpm` (31.5 MiB), `vdf_0.1.0_amd64.AppImage` (126.7 MiB). CI adds
Windows MSI/portable (MinGW route — first CI cycle will validate) and the Arch package.

## Manual validation performed

- Release binary launched on the desktop (Wayland): alive, clean stderr.
- The complete open→render→navigate pipeline is validated headlessly (e2e test drives
  the exact `AppCore`/`DocCore` methods the commands call).
- A generated 30-page sample PDF is at `~/Documents/vdf-sample-30pages.pdf` for manual
  checking (see checklist). GUI interaction (mouse wheel, pinch feel) was NOT
  validated by automation — no computer-use available in this session; it is the
  first item of the user checklist below.

## Performance results

Baselines in `bench/baselines.json` (ungated). Headline numbers on the 12-core/14 GiB
machine: 1000-page open ~0.2 ms; first viewport ~21 ms; 256-px tile of a distant page
~8 ms; zoom re-layout 0.13 ms. MuPDF rasterization of a full A4 page at zoom 1 is
~4 ms (fixture content; real documents vary).

## Problems & fixes

Summarized in the tests section. The one debugging rabbit hole: a loader-thread stall
that ptrace/gdb/eu-stack could not inspect in this sandbox — resolved with file-based
event tracing, which exposed the unwritten `last_viewport` retry field.

## Known limitations

- **GUI interaction feel is unvalidated by automation** (no computer-use in session);
  the manual checklist covers it.
- Windows CI uses the MinGW make route (plan §15); first CI cycle will confirm.
- Encrypted PDFs: detected and reported, but no unlock dialog yet (M5). Signed-PDF
  awareness is M5 scope per plan.
- Thumbnails: loader-rendered, windowed around the active page; the panel is not
  fully virtualized for 5000-page docs yet (M7).
- Three-finger pinch: delivered through the same document-only zoom path as other
  pinch inputs (WebKitGTK exposes trackpad pinch as Ctrl+wheel); no UI zoom exists to
  accidentally scale. Feel validated manually in M2/M7 on real hardware.
- `mupdf-sys` links `libmupdf-third.a` wholesale (~57 MiB static); size optimization
  (feature-trimmed MuPDF build) is an M7 item.

## Manual validation checklist for the user

1. `npm run tauri dev` (or install the fresh package) → `Ctrl+O` → open
   `~/Documents/vdf-sample-30pages.pdf` → **pages render as tiles** (colored marks
   top-right of each page, "Page N" text).
2. Scroll (wheel/drag): smooth; fast scrollbar-style jumps fill in progressively
   without blank frames.
3. Ctrl+wheel over the page: zoom centered on the cursor; content stays visible while
   sharper tiles arrive (no black/white flash).
4. Ctrl+B → Pages panel: thumbnails appear around the current page; click one to jump.
   Outline panel shows "(no outline)" for the sample.
5. Ctrl+1 (fit width), Ctrl+0 (100%), Ctrl+R (rotate), palette "Page Mode: Two Pages".
6. Status bar shows Page N/30 and live tile-cache counters.
7. Try a real-world PDF (the sample is synthetic) — text-heavy pages are the real
   rendering test.

## Final commit hash

Milestone commit: `0610d18cf83db53e8e8faf73844892b5a143f5e8` on `main` — contains the complete M1 implementation and this
report. CI verification results are recorded below after the push.
