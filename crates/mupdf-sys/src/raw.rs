//! Raw FFI declarations for MuPDF 1.28.5.
//!
//! Struct layouts mirror `include/mupdf/fitz/geometry.h` and
//! `include/mupdf/fitz/outline.h` exactly (verified against the pinned
//! submodule). Fallible calls are NOT declared here directly — use the
//! `vdf_*` shims from `shim.c`, which keep MuPDF's longjmp in C.

use std::ffi::{c_char, c_int, c_uint, c_void};

/// `fz_matrix` — `[a b c d e f]`, row-major as in MuPDF headers.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct fz_matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

/// `fz_point`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct fz_point {
    pub x: f32,
    pub y: f32,
}

/// `fz_rect`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct fz_rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// `fz_irect`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct fz_irect {
    pub x0: c_int,
    pub y0: c_int,
    pub x1: c_int,
    pub y1: c_int,
}

/// `fz_location`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct fz_location {
    pub chapter: c_int,
    pub page: c_int,
}

/// `fz_locks_context`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct fz_locks_context {
    pub user: *mut c_void,
    pub lock: Option<extern "C" fn(*mut c_void, c_int)>,
    pub unlock: Option<extern "C" fn(*mut c_void, c_int)>,
}

/// `fz_outline` (subset we walk: the full struct has color bitfields we
/// never read; fields up to `down` are layout-compatible).
#[repr(C)]
pub struct fz_outline {
    pub refs: c_int,
    pub title: *mut c_char,
    pub uri: *mut c_char,
    pub page: fz_location,
    pub x: f32,
    pub y: f32,
    pub next: *mut fz_outline,
    pub down: *mut fz_outline,
    pub _rest: [c_uint; 1],
}

// Opaque MuPDF types.
#[repr(C)]
pub struct fz_context {
    _private: [u8; 0],
}
#[repr(C)]
pub struct fz_document {
    _private: [u8; 0],
}
#[repr(C)]
pub struct fz_page {
    _private: [u8; 0],
}
#[repr(C)]
pub struct fz_device {
    _private: [u8; 0],
}
#[repr(C)]
pub struct fz_pixmap {
    _private: [u8; 0],
}
#[repr(C)]
pub struct fz_colorspace {
    _private: [u8; 0],
}
#[repr(C)]
pub struct fz_stream {
    _private: [u8; 0],
}
#[repr(C)]
pub struct fz_separations {
    _private: [u8; 0],
}
#[repr(C)]
pub struct fz_cookie {
    _private: [u8; 0],
}
#[repr(C)]
pub struct fz_display_list {
    _private: [u8; 0],
}

unsafe extern "C" {
    // Infallible (no fz_throw): safe to declare directly.
    // NOTE: `fz_new_context` is a C macro expanding to fz_new_context_imp
    // with the FZ_VERSION string; the archive only exports the _imp symbol.
    pub fn fz_new_context_imp(
        alloc: *const c_void,
        locks: *const fz_locks_context,
        store_max: usize,
        version: *const c_char,
    ) -> *mut fz_context;
    pub fn fz_drop_context(ctx: *mut fz_context);
    pub fn fz_clone_context(ctx: *mut fz_context) -> *mut fz_context;

    pub fn fz_drop_document(ctx: *mut fz_context, doc: *mut fz_document);
    pub fn fz_needs_password(ctx: *mut fz_context, doc: *mut fz_document) -> c_int;
    pub fn fz_drop_page(ctx: *mut fz_context, page: *mut fz_page);
    pub fn fz_drop_device(ctx: *mut fz_context, dev: *mut fz_device);
    pub fn fz_drop_pixmap(ctx: *mut fz_context, pix: *mut fz_pixmap);
    pub fn fz_drop_stream(ctx: *mut fz_context, stm: *mut fz_stream);
    pub fn fz_drop_outline(ctx: *mut fz_context, outline: *mut fz_outline);

    pub fn fz_pixmap_width(ctx: *mut fz_context, pix: *const fz_pixmap) -> c_int;
    pub fn fz_pixmap_height(ctx: *mut fz_context, pix: *const fz_pixmap) -> c_int;
    pub fn fz_pixmap_samples(ctx: *mut fz_context, pix: *const fz_pixmap) -> *mut u8;
    pub fn fz_device_rgb(ctx: *mut fz_context) -> *mut fz_colorspace;
    pub fn fz_open_memory(ctx: *mut fz_context, data: *const u8, len: usize) -> *mut fz_stream;
    pub fn fz_register_document_handlers(ctx: *mut fz_context);
    pub fn fz_drop_display_list(ctx: *mut fz_context, list: *mut fz_display_list);
}

// C shims from src/shim.c (fallible calls; contain fz_try in C).
unsafe extern "C" {
    pub fn vdf_open_document(
        ctx: *mut fz_context,
        path: *const c_char,
        out: *mut *mut fz_document,
        errbuf: *mut c_char,
        errlen: usize,
    ) -> c_int;
    pub fn vdf_open_document_with_stream(
        ctx: *mut fz_context,
        magic: *const c_char,
        stm: *mut fz_stream,
        out: *mut *mut fz_document,
        errbuf: *mut c_char,
        errlen: usize,
    ) -> c_int;
    pub fn vdf_count_pages(
        ctx: *mut fz_context,
        doc: *mut fz_document,
        out: *mut c_int,
        errbuf: *mut c_char,
        errlen: usize,
    ) -> c_int;
    pub fn vdf_authenticate_password(
        ctx: *mut fz_context,
        doc: *mut fz_document,
        password: *const c_char,
        out: *mut c_int,
        errbuf: *mut c_char,
        errlen: usize,
    ) -> c_int;
    pub fn vdf_load_page(
        ctx: *mut fz_context,
        doc: *mut fz_document,
        number: c_int,
        out: *mut *mut fz_page,
        errbuf: *mut c_char,
        errlen: usize,
    ) -> c_int;
    pub fn vdf_bound_page(
        ctx: *mut fz_context,
        page: *mut fz_page,
        out: *mut fz_rect,
        errbuf: *mut c_char,
        errlen: usize,
    ) -> c_int;
    pub fn vdf_render_tile(
        ctx: *mut fz_context,
        page: *mut fz_page,
        cs: *mut fz_colorspace,
        ctm: fz_matrix,
        bbox: fz_irect,
        alpha: c_int,
        out: *mut *mut fz_pixmap,
        errbuf: *mut c_char,
        errlen: usize,
    ) -> c_int;
    pub fn vdf_new_draw_device(
        ctx: *mut fz_context,
        ctm: fz_matrix,
        dest: *mut fz_pixmap,
        out: *mut *mut fz_device,
        errbuf: *mut c_char,
        errlen: usize,
    ) -> c_int;
    pub fn vdf_new_pixmap(
        ctx: *mut fz_context,
        cs: *mut fz_colorspace,
        bbox: fz_irect,
        alpha: c_int,
        out: *mut *mut fz_pixmap,
        errbuf: *mut c_char,
        errlen: usize,
    ) -> c_int;
    pub fn vdf_load_outline(
        ctx: *mut fz_context,
        doc: *mut fz_document,
        out: *mut *mut fz_outline,
        errbuf: *mut c_char,
        errlen: usize,
    ) -> c_int;
    pub fn vdf_render_list_tile(
        ctx: *mut fz_context,
        list: *mut fz_display_list,
        cs: *mut fz_colorspace,
        ctm: fz_matrix,
        bbox: fz_irect,
        alpha: c_int,
        out: *mut *mut fz_pixmap,
        errbuf: *mut c_char,
        errlen: usize,
    ) -> c_int;
    pub fn vdf_new_display_list_from_page(
        ctx: *mut fz_context,
        page: *mut fz_page,
        out: *mut *mut fz_display_list,
        errbuf: *mut c_char,
        errlen: usize,
    ) -> c_int;
    pub fn vdf_call_protected(
        ctx: *mut fz_context,
        cb: extern "C" fn(*mut fz_context, *mut c_void),
        user: *mut c_void,
        errbuf: *mut c_char,
        errlen: usize,
    ) -> c_int;
}
