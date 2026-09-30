# ADR 0002 — M1 viewer decisions

Status: Accepted (2026-10-01)
Context: MASTER_PLAN.md §8, §10, §16/M1. Records the decisions taken while building
the PDF viewer; supersedes nothing, amends ADR 0001 where noted.

## Decisions

1. **Display lists are the concurrency unit.** MuPDF multi-threading rule 2 forbids
   simultaneous document access; only finished display lists may run concurrently.
   The loader thread (document actor role) builds them serially; render workers run
   them. This replaces the earlier plan wording "render workers access the document
   with cloned contexts" — cloned contexts yes, but only against display lists.
2. **C shims contain all fz_try/fz_catch.** MuPDF's setjmp/longjmp error handling
   must never unwind across the FFI boundary; every fallible call has a C wrapper
   returning 0/1 + message buffer (crates/mupdf-sys/src/shim.c).
3. **Loader thread instead of a full command-actor in M1.** All serial document work
   (page-size sweep, display-list builds, thumbnails) runs on one per-document
   thread. The full document actor arrives with editing commands in M2/M3; the
   seam (DocCore methods shared by commands and tests) is already in place.
4. **No-blanking via compositor-side fallback.** When a tile slot's current-step
   render is pending, the compositor draws the nearest lower-step tile scaled
   (~9% max upscale at the 2^(1/8) ladder). Old textures persist GPU-side until
   evicted (LRU, 512 cap).
5. **Tile transport = Tauri custom URI scheme, 16-byte header + raw RGBA.**
   Fetched only for keys the per-frame poll reports ready. Windows uses the
   http://vdf-tile.localhost host form; Linux/macOS the vdf-tile:// form.
6. **Windows builds via MinGW make** (plan §15 route): MSYS2 gcc/make builds MuPDF;
   the Rust side uses the windows-gnu toolchain so the CRT matches. MSVC + MinGW
   C++ statics (harfbuzz) would mix runtimes — rejected without testing.
7. **Zoom quantization ladder 2^(1/8)** with continuous display zoom (from ADR 0001,
   now implemented): tiles snap to steps; the compositor scales by
   zoom / zoom_for_step(≤ +9%).

## Consequences

- The scheduler's TileRenderer trait hides whether rendering goes through a display
  list (production) or a fake (tests) — M2 ink rendering plugs in the same way.
- Loader-thread serialization means a slow display-list build delays thumbnails for
  other pages; acceptable until M7 (worker prioritization there).
- The `last_viewport` retry contract (loader → scheduler re-push) must be kept by
  any new viewport entry point.
