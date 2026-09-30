//! Binary transports: `vdf-tile://` and `vdf-thumb://` custom URI schemes.
//!
//! Response layout (tiles): 16-byte header — magic "VDFT", version u8,
//! flags u8, width u16 LE, height u16 LE, reserved u32 — followed by raw
//! RGBA8 pixels. No PNG, no base64, no JSON in the hot path (plan §6).
//! Thumbnails: raw RGBA8 (dimensions arrive via the poll command).

use std::sync::Arc;

use tauri::http::{Response, StatusCode};
use tauri::{Manager, State, UriSchemeContext};

use crate::commands::parse_tile_key;
use crate::state::AppCore;

const TILE_HEADER_LEN: usize = 16;

fn tile_response(pixels: &[u8], w: u32, h: u32) -> Response<Vec<u8>> {
    let mut body = Vec::with_capacity(TILE_HEADER_LEN + pixels.len());
    body.extend_from_slice(b"VDFT");
    body.push(1); // version
    body.push(0); // flags
    body.extend_from_slice(&(w as u16).to_le_bytes());
    body.extend_from_slice(&(h as u16).to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes()); // reserved
    body.extend_from_slice(pixels);
    let mut resp = Response::new(body);
    resp.headers_mut().insert(
        "content-type",
        "application/octet-stream".parse().expect("static header"),
    );
    resp.headers_mut()
        .insert("cache-control", "no-store".parse().expect("static header"));
    resp
}

fn error_response(code: StatusCode, msg: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(code)
        .body(msg.as_bytes().to_vec())
        .expect("static response")
}

/// Handles `vdf-tile://localhost/p{page}z{step}r{rot}x{x}y{y}`.
pub fn handle_tile_protocol<R: tauri::Runtime>(
    state: &State<AppCore>,
    _ctx: &UriSchemeContext<'_, R>,
    request: tauri::http::Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let uri = request.uri().to_string();
    let key_part = uri
        .rsplit('/')
        .next()
        .unwrap_or("")
        .split('?')
        .next()
        .unwrap_or("");
    let Some(id) = doc_id_from_uri(&uri) else {
        return error_response(StatusCode::BAD_REQUEST, "no doc id");
    };
    let Ok(core) = state.docs.lock().unwrap().get(&id).cloned().ok_or(()) else {
        return error_response(StatusCode::NOT_FOUND, "document closed");
    };
    let Some(key) = parse_tile_key(key_part) else {
        return error_response(StatusCode::BAD_REQUEST, "bad tile key");
    };
    match core.scheduler.tile_pixels(&key) {
        Some((w, h, px)) => tile_response(&px, w, h),
        None => error_response(StatusCode::NOT_FOUND, "tile not ready"),
    }
}

/// Handles `vdf-thumb://localhost/{doc}/{page}/{width}` — 404 until the
/// loader thread has rendered it.
pub fn handle_thumb_protocol<R: tauri::Runtime>(
    state: &State<AppCore>,
    _ctx: &UriSchemeContext<'_, R>,
    request: tauri::http::Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let uri = request.uri().to_string();
    let path = uri.split("//").nth(1).unwrap_or("");
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() < 3 {
        return error_response(StatusCode::BAD_REQUEST, "expected /{doc}/{page}/{width}");
    }
    let (Some(id), Some(page), Some(width)) = (
        parts[0].parse::<u64>().ok(),
        parts[1].parse::<u32>().ok(),
        parts[2].parse::<u32>().ok(),
    ) else {
        return error_response(StatusCode::BAD_REQUEST, "bad numbers");
    };
    let Some(core) = state.docs.lock().unwrap().get(&id).cloned() else {
        return error_response(StatusCode::NOT_FOUND, "document closed");
    };
    match core.thumb(page, width) {
        Some(px) => {
            let mut resp = Response::new((*px).clone());
            resp.headers_mut().insert(
                "content-type",
                "application/octet-stream".parse().expect("static header"),
            );
            resp
        }
        None => error_response(StatusCode::NOT_FOUND, "thumbnail not ready"),
    }
}

fn doc_id_from_uri(uri: &str) -> Option<u64> {
    // host part carries the doc id: vdf-tile://localhost/... on Linux,
    // http://vdf-tile.<id>.localhost/... style on some platforms.
    // We keep it simple and put the id in the path: /d{doc}/{key}.
    let path = uri.split("//").nth(1).unwrap_or("");
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    segs.first()?.strip_prefix('d')?.parse().ok()
}

/// Registers both protocols on the Tauri builder.
pub fn register(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder
        .register_uri_scheme_protocol("vdf-tile", |ctx, request| {
            let state: State<AppCore> = ctx.app_handle().state();
            handle_tile_protocol(&state, &ctx, request)
        })
        .register_uri_scheme_protocol("vdf-thumb", |ctx, request| {
            let state: State<AppCore> = ctx.app_handle().state();
            handle_thumb_protocol(&state, &ctx, request)
        })
}

/// Shared-pixel helper for tests.
pub fn _assert_rgba_len(w: u32, h: u32, px: &Arc<Vec<u8>>) -> bool {
    px.len() == (w * h * 4) as usize
}
