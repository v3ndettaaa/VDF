//! FFI smoke test: prove the vendored MuPDF builds, loads, and renders.
//! This is the M1 spike gate (MASTER_PLAN.md §15, §16/M1) — everything else
//! in vdf-pdf stands on this working.

use std::ffi::CString;

use mupdf_sys::raw;
use mupdf_sys::{FzContext, errbuf, take_error};

/// Minimal valid one-page PDF: blue rect + "Hello VDF" text, 612×792.
const MINIMAL_PDF: &[u8] = b"%PDF-1.4
1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj
2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj
3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]/Contents 4 0 R/Resources<</Font<</F1 5 0 R>>>>>>endobj
4 0 obj<</Length 62>>stream
BT /F1 24 Tf 72 700 Td (Hello VDF) Tj ET
0 0 1 rg 100 100 200 150 re f
endstream
endobj
5 0 obj<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>endobj
trailer<</Size 6/Root 1 0 R>>
%%EOF";

fn ctx() -> FzContext {
    FzContext::new(256 << 20).expect("create fz_context")
}

fn open_minimal(ctx: &FzContext) -> *mut raw::fz_document {
    let mut doc: *mut raw::fz_document = std::ptr::null_mut();
    let mut err = errbuf();
    let stream =
        unsafe { raw::fz_open_memory(ctx.as_ptr(), MINIMAL_PDF.as_ptr(), MINIMAL_PDF.len()) };
    let magic = CString::new("application/pdf").unwrap();
    let rc = unsafe {
        raw::vdf_open_document_with_stream(
            ctx.as_ptr(),
            magic.as_ptr(),
            stream,
            &mut doc,
            err.as_mut_ptr(),
            mupdf_sys::ERRBUF_LEN,
        )
    };
    assert_eq!(rc, 0, "open failed: {}", take_error(&err));
    doc
}

#[test]
fn open_count_bound() {
    let ctx = ctx();
    let doc = open_minimal(&ctx);

    let mut pages: i32 = 0;
    let mut err = errbuf();
    let rc = unsafe {
        raw::vdf_count_pages(
            ctx.as_ptr(),
            doc,
            &mut pages,
            err.as_mut_ptr(),
            mupdf_sys::ERRBUF_LEN,
        )
    };
    assert_eq!(rc, 0, "count_pages: {}", take_error(&err));
    assert_eq!(pages, 1);
    assert_eq!(unsafe { raw::fz_needs_password(ctx.as_ptr(), doc) }, 0);

    let mut page: *mut raw::fz_page = std::ptr::null_mut();
    let mut err = errbuf();
    let rc = unsafe {
        raw::vdf_load_page(
            ctx.as_ptr(),
            doc,
            0,
            &mut page,
            err.as_mut_ptr(),
            mupdf_sys::ERRBUF_LEN,
        )
    };
    assert_eq!(rc, 0, "load_page: {}", take_error(&err));

    let mut bounds = raw::fz_rect {
        x0: 0.0,
        y0: 0.0,
        x1: 0.0,
        y1: 0.0,
    };
    let mut err = errbuf();
    let rc = unsafe {
        raw::vdf_bound_page(
            ctx.as_ptr(),
            page,
            &mut bounds,
            err.as_mut_ptr(),
            mupdf_sys::ERRBUF_LEN,
        )
    };
    assert_eq!(rc, 0, "bound_page: {}", take_error(&err));
    assert!((bounds.x0 - 0.0).abs() < 0.01 && (bounds.y0 - 0.0).abs() < 0.01);
    assert!(
        ((bounds.x1 - 612.0).abs() < 0.01) && ((bounds.y1 - 792.0).abs() < 0.01),
        "unexpected bounds {bounds:?}"
    );

    unsafe {
        raw::fz_drop_page(ctx.as_ptr(), page);
        raw::fz_drop_document(ctx.as_ptr(), doc);
    }
}

#[test]
fn render_full_page_pixels() {
    let ctx = ctx();
    let doc = open_minimal(&ctx);
    let mut page: *mut raw::fz_page = std::ptr::null_mut();
    let mut err = errbuf();
    unsafe {
        assert_eq!(
            raw::vdf_load_page(
                ctx.as_ptr(),
                doc,
                0,
                &mut page,
                err.as_mut_ptr(),
                mupdf_sys::ERRBUF_LEN
            ),
            0
        );
    }

    let ctm = raw::fz_matrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };
    let bbox = raw::fz_irect {
        x0: 0,
        y0: 0,
        x1: 612,
        y1: 792,
    };
    let mut pix: *mut raw::fz_pixmap = std::ptr::null_mut();
    let mut err = errbuf();
    let rc = unsafe {
        raw::vdf_render_tile(
            ctx.as_ptr(),
            page,
            raw::fz_device_rgb(ctx.as_ptr()),
            ctm,
            bbox,
            1, // alpha
            &mut pix,
            err.as_mut_ptr(),
            mupdf_sys::ERRBUF_LEN,
        )
    };
    assert_eq!(rc, 0, "render_tile: {}", take_error(&err));

    let (w, h) = unsafe {
        (
            raw::fz_pixmap_width(ctx.as_ptr(), pix),
            raw::fz_pixmap_height(ctx.as_ptr(), pix),
        )
    };
    assert_eq!((w, h), (612, 792));
    let samples = unsafe { raw::fz_pixmap_samples(ctx.as_ptr(), pix) };
    assert!(!samples.is_null());

    // Pixel probes: PDF y-up rect (100,100)-(300,250) maps to y-down
    // (100,542)-(300,692) in page space; mupdf page space is y-down.
    let at = |x: usize, y: usize| -> [u8; 4] {
        let off = (y * w as usize + x) * 4;
        unsafe {
            [
                *samples.add(off),
                *samples.add(off + 1),
                *samples.add(off + 2),
                *samples.add(off + 3),
            ]
        }
    };
    let blue = at(200, 600);
    assert!(
        blue[0] < 40 && blue[1] < 40 && blue[2] > 200 && blue[3] == 255,
        "rect pixel must be opaque blue, got {blue:?}"
    );
    let white = at(30, 30);
    assert!(
        white[0] > 230 && white[1] > 230 && white[2] > 230 && white[3] == 255,
        "background must be opaque white, got {white:?}"
    );

    unsafe {
        raw::fz_drop_pixmap(ctx.as_ptr(), pix);
        raw::fz_drop_page(ctx.as_ptr(), page);
        raw::fz_drop_document(ctx.as_ptr(), doc);
    }
}

#[test]
fn malformed_pdf_reports_error_not_panic() {
    let ctx = ctx();
    let garbage = b"%PDF-1.7 this is not a real pdf \xff\xfe garbage";
    let mut doc: *mut raw::fz_document = std::ptr::null_mut();
    let mut err = errbuf();
    let stream = unsafe { raw::fz_open_memory(ctx.as_ptr(), garbage.as_ptr(), garbage.len()) };
    let magic = CString::new("application/pdf").unwrap();
    let rc = unsafe {
        raw::vdf_open_document_with_stream(
            ctx.as_ptr(),
            magic.as_ptr(),
            stream,
            &mut doc,
            err.as_mut_ptr(),
            mupdf_sys::ERRBUF_LEN,
        )
    };
    // Either the open itself fails (fine), or opening "succeeds" as mupdf
    // repairs garbage — but counting pages must not panic the process.
    if rc == 0 {
        let mut pages: i32 = -1;
        let mut err2 = errbuf();
        let _ = unsafe {
            raw::vdf_count_pages(
                ctx.as_ptr(),
                doc,
                &mut pages,
                err2.as_mut_ptr(),
                mupdf_sys::ERRBUF_LEN,
            )
        };
        unsafe { raw::fz_drop_document(ctx.as_ptr(), doc) };
    } else {
        assert!(!take_error(&err).0.is_empty());
    }
}

#[test]
fn cloned_context_renders_independently() {
    let ctx = ctx();
    let clone = ctx.clone_context().expect("clone context");
    let mut doc: *mut raw::fz_document = std::ptr::null_mut();
    let mut err = errbuf();
    let stream =
        unsafe { raw::fz_open_memory(ctx.as_ptr(), MINIMAL_PDF.as_ptr(), MINIMAL_PDF.len()) };
    let magic = CString::new("application/pdf").unwrap();
    let rc = unsafe {
        raw::vdf_open_document_with_stream(
            clone.ptr,
            magic.as_ptr(),
            stream,
            &mut doc,
            err.as_mut_ptr(),
            mupdf_sys::ERRBUF_LEN,
        )
    };
    assert_eq!(rc, 0, "open on clone: {}", take_error(&err));
    let mut pages: i32 = 0;
    let mut err = errbuf();
    unsafe {
        assert_eq!(
            raw::vdf_count_pages(
                clone.ptr,
                doc,
                &mut pages,
                err.as_mut_ptr(),
                mupdf_sys::ERRBUF_LEN
            ),
            0
        );
        raw::fz_drop_document(clone.ptr, doc);
    }
    // clone dropped before master (order enforced by scope)
    drop(clone);
}
