# VDF

**VDF** is a professional native desktop PDF editor built with Rust, Tauri v2, and a
framework-free TypeScript UI, using MuPDF as the PDF engine.

Priorities: smooth interaction, low input latency, large-PDF performance (1000+ pages),
stability, low memory, professional editing, reliable persistence/recovery, and strong
automated testing on Windows, Linux, and Arch.

## Status

**M0 — Foundation** (complete). The workspace, Tauri shell, UI shell, document-model
foundations, command/undo history, rendering/input transport abstractions, atomic-write
persistence primitives, test infrastructure, CI, and packaging are in place. PDF
integration (MuPDF) begins in M1 — M0 intentionally does not parse or render PDFs.

Roadmap: M0 foundation → M1 PDF viewer → M2 handwriting → M3 editing → M4 search/OCR →
M5 persistence/security → M6 product UI → M7 performance → M8 packaging/release.
See [MASTER_PLAN.md](MASTER_PLAN.md) for the full architecture and milestone plan and
[reports/](reports/) for milestone reports.

## Repository layout

```text
crates/           Rust workspace: vdf-core, vdf-document, vdf-pdf, vdf-render,
                  vdf-input, vdf-persist, vdf-search, vdf-diag, vdf-bench
src-tauri/        vdf-app — the Tauri v2 shell (window, dialogs, IPC)
ui/               framework-free TypeScript frontend (Vite is bundler only)
tests/            cross-crate fixtures and golden vectors
fuzz/             cargo-fuzz targets
packaging/arch/   PKGBUILD for the native Arch package (.pkg.tar.zst)
reports/          milestone reports
```

## Development

Prerequisites: Rust 1.98.1, Node 22+, and the Tauri v2 system dependencies
(libwebkit2gtk-4.1-dev, libgtk-3-dev on Linux; see the Tauri docs for Windows).

```sh
npm install          # frontend deps
npm run dev          # UI dev server (vite)
npm run tauri dev    # full desktop app in dev mode
npm run tauri build  # release bundles (deb/appimage/msi per platform)
```

### Stable test commands (never renamed)

| Command | What it runs |
|---|---|
| `npm test` | unit + integration |
| `npm run test:unit` | Rust lib tests + vitest |
| `npm run test:integration` | cross-crate integration tests |
| `npm run test:render` | rendering tests (M1) |
| `npm run test:e2e` | end-to-end tests (M6) |
| `npm run test:performance` | vdf-bench smoke scenario |
| `npm run test:fuzz` | cargo-fuzz smoke (needs nightly + cargo-fuzz) |
| `npm run typecheck` | `tsc --noEmit` |

## License

AGPL-3.0-only. VDF uses MuPDF, which is AGPL-licensed; this keeps distribution legal
(see [MASTER_PLAN.md](MASTER_PLAN.md) §3).
