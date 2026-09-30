//! The vdf:* command surface (M1 viewer scope). JSON commands are for
//! control-plane data only — tiles and thumbnails travel as raw bytes over
//! the custom URI protocols (see protocols.rs).

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use vdf_core::{PageIndex, Rotation};
use vdf_render::PageMode;

use crate::state::{AppCore, DocCore};

#[derive(Serialize)]
pub struct DocInfo {
    pub id: u64,
    pub page_count: u32,
    pub first_page: (f64, f64),
    pub path: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct LayoutPageJson {
    pub index: u32,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Serialize)]
pub struct LayoutJson {
    pub rev: u64,
    pub mode: String,
    pub rotation: u32,
    pub doc_w: f64,
    pub doc_h: f64,
    pub pages: Vec<LayoutPageJson>,
}

#[derive(Serialize)]
pub struct PollResult {
    pub scroll: (f64, f64),
    pub zoom: f64,
    pub zoom_step: u16,
    pub active_page: u32,
    /// Layout JSON when its revision changed since the last poll.
    pub layout: Option<LayoutJson>,
    /// Tile keys that became ready since the last poll.
    pub ready_tiles: Vec<String>,
    /// (page, w, h) thumbnails ready since the last poll.
    pub thumbs_ready: Vec<(u32, u32, u32)>,
    /// Scheduler health snapshot.
    pub cache_tiles: usize,
    pub cache_bytes: usize,
    pub rendered_total: u64,
    pub stale_dropped: u64,
}

fn doc(state: &State<AppCore>, id: u64) -> Result<Arc<DocCore>, String> {
    state
        .docs
        .lock()
        .map_err(|e| e.to_string())?
        .get(&id)
        .cloned()
        .ok_or_else(|| format!("no document {id}"))
}

fn layout_json(core: &DocCore, slot: &vdf_render::DocumentLayout) -> LayoutJson {
    LayoutJson {
        rev: core.layout_rev(),
        mode: format!("{:?}", *core.mode.lock().unwrap()).to_lowercase(),
        rotation: core.rotation.lock().unwrap().quarter_turns(),
        doc_w: slot.doc_size.0,
        doc_h: slot.doc_size.1,
        pages: slot
            .pages
            .iter()
            .map(|p| LayoutPageJson {
                index: p.index.0,
                x: p.rect.min.x,
                y: p.rect.min.y,
                w: p.rect.width(),
                h: p.rect.height(),
            })
            .collect(),
    }
}

/// Opens a document from a path (reads the bytes; never writes the file).
#[tauri::command]
pub fn vdf_open_document(state: State<AppCore>, path: String) -> Result<DocInfo, String> {
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let name = std::path::Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.clone());
    match state.open_document(&path, bytes) {
        Ok(core) => {
            let first = *core
                .page_sizes
                .lock()
                .unwrap()
                .first()
                .unwrap_or(&(612.0, 792.0));
            Ok(DocInfo {
                id: core.handle.0,
                page_count: core.page_count,
                first_page: first,
                path: path.clone(),
                name,
            })
        }
        Err(e) => Err(e.to_string()),
    }
}
#[tauri::command]
pub fn vdf_close_document(state: State<AppCore>, id: u64) -> Result<(), String> {
    state.close_document(id);
    Ok(())
}

/// Viewport update from the UI (scroll/zoom/resize). Cheap; enqueues DL
/// loads for visible pages and reprioritizes tiles.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // flat IPC argument list by design
pub fn vdf_viewport(
    state: State<AppCore>,
    id: u64,
    viewport_w: f64,
    viewport_h: f64,
    dpr: f64,
    scroll_x: f64,
    scroll_y: f64,
    velocity_y: f64,
    interacting: bool,
) -> Result<(), String> {
    let core = doc(&state, id)?;
    *core.viewport_px.lock().unwrap() = (viewport_w, viewport_h);
    *core.dpr.lock().unwrap() = dpr;
    {
        let mut z = core.zoom.lock().unwrap();
        z.set_scroll(scroll_x, scroll_y);
    }
    core.clamp_scroll();
    core.update_active_page();
    let visible = core.visible_rect();
    core.ensure_display_lists(visible);
    core.scheduler
        .update_viewport(core.viewport_state(velocity_y, interacting));
    Ok(())
}

/// Per-frame poll: authoritative view state + ready tiles + layout.
#[tauri::command]
pub fn vdf_poll(
    state: State<AppCore>,
    id: u64,
    layout_rev_seen: u64,
) -> Result<PollResult, String> {
    let core = doc(&state, id)?;
    let update = core.scheduler.drain_results();
    let st_stats = core.scheduler.cache_stats();
    let layout_slot = core.layout.lock().unwrap();
    let layout = if layout_slot.rev != layout_rev_seen {
        Some(layout_json(&core, &layout_slot.layout))
    } else {
        None
    };
    drop(layout_slot);
    Ok(PollResult {
        scroll: core.zoom.lock().unwrap().scroll(),
        zoom: core.zoom.lock().unwrap().zoom(),
        zoom_step: vdf_core::tile::zoom_step_for(core.zoom.lock().unwrap().zoom()),
        active_page: core.active_page.load(std::sync::atomic::Ordering::Relaxed) as u32,
        layout,
        ready_tiles: update.ready.iter().map(tile_key_str).collect(),
        thumbs_ready: core.take_thumb_ready(),
        cache_tiles: st_stats.0,
        cache_bytes: st_stats.1,
        rendered_total: core.scheduler.rendered_total(),
        stale_dropped: update.stale_dropped,
    })
}

pub fn tile_key_str(key: &vdf_core::TileKey) -> String {
    format!(
        "p{}z{}r{}x{}y{}",
        key.page.0,
        key.zoom_step,
        key.rotation.quarter_turns(),
        key.x,
        key.y
    )
}

/// Zoom via the central controller. Focal point + viewport size in device px.
#[tauri::command]
pub fn vdf_zoom(
    state: State<AppCore>,
    id: u64,
    factor: f64,
    focal_x: f64,
    focal_y: f64,
    absolute: Option<f64>,
) -> Result<(), String> {
    let core = doc(&state, id)?;
    let vp = *core.viewport_px.lock().unwrap();
    let mut z = core.zoom.lock().unwrap();
    if let Some(target) = absolute {
        z.set_zoom_focal(target, (focal_x, focal_y), vp);
    } else {
        let cur = z.zoom();
        z.set_zoom_focal(cur * factor, (focal_x, focal_y), vp);
    }
    drop(z);
    core.rebuild_layout();
    core.clamp_scroll();
    Ok(())
}

/// Pans the view (dx, dy in device px), clamped.
#[tauri::command]
pub fn vdf_pan(state: State<AppCore>, id: u64, dx: f64, dy: f64) -> Result<(), String> {
    let core = doc(&state, id)?;
    let mut z = core.zoom.lock().unwrap();
    let (sx, sy) = z.scroll();
    z.set_scroll(sx + dx, sy + dy);
    drop(z);
    core.clamp_scroll();
    Ok(())
}

/// Sets the page mode and re-lays-out.
#[tauri::command]
pub fn vdf_set_page_mode(state: State<AppCore>, id: u64, mode: String) -> Result<(), String> {
    let core = doc(&state, id)?;
    let mode = match mode.as_str() {
        "continuous" => PageMode::Continuous,
        "single" => PageMode::Single,
        "two-page" => PageMode::TwoPage,
        other => return Err(format!("unknown mode {other}")),
    };
    *core.mode.lock().unwrap() = mode;
    core.rebuild_layout();
    core.clamp_scroll();
    Ok(())
}

/// Rotates the view (quarter turns clockwise from the current).
#[tauri::command]
pub fn vdf_rotate(state: State<AppCore>, id: u64, quarter_turns: u32) -> Result<(), String> {
    let core = doc(&state, id)?;
    let cur = core.rotation.lock().unwrap().quarter_turns();
    *core.rotation.lock().unwrap() = Rotation::from_quarter_turns(cur + quarter_turns);
    core.rebuild_layout();
    core.clamp_scroll();
    Ok(())
}

/// Document outline (empty when the file has none).
#[derive(Serialize)]
pub struct OutlineJson {
    pub title: String,
    pub page: Option<u32>,
    pub children: Vec<OutlineJson>,
}

fn outline_json(entries: &[vdf_pdf::OutlineEntry]) -> Vec<OutlineJson> {
    entries
        .iter()
        .map(|e| OutlineJson {
            title: e.title.clone(),
            page: e.page,
            children: outline_json(&e.children),
        })
        .collect()
}

#[tauri::command]
pub fn vdf_outline(state: State<AppCore>, id: u64) -> Result<Vec<OutlineJson>, String> {
    let core = doc(&state, id)?;
    let tree = core.doc.outline().map_err(|e| e.to_string())?;
    Ok(outline_json(&tree))
}

/// Scroll so a page is visible; returns the target scroll.
#[tauri::command]
pub fn vdf_goto_page(state: State<AppCore>, id: u64, page: u32) -> Result<(f64, f64), String> {
    let core = doc(&state, id)?;
    if page >= core.page_count {
        return Err(format!("page {page} out of range"));
    }
    let target = core.page_scroll_target(page).ok_or("no layout")?;
    {
        let mut z = core.zoom.lock().unwrap();
        z.set_scroll(target.0, target.1);
    }
    core.clamp_scroll();
    core.update_active_page();
    let visible = core.visible_rect();
    core.ensure_display_lists(visible);
    core.scheduler
        .update_viewport(core.viewport_state(0.0, false));
    Ok(target)
}

/// Requests thumbnails (loader thread renders them in the background).
#[tauri::command]
pub fn vdf_request_thumbnails(
    state: State<AppCore>,
    id: u64,
    pages: Vec<u32>,
    width: u32,
) -> Result<(), String> {
    use crate::state::LoaderMsg;
    let core = doc(&state, id)?;
    for page in pages {
        let _ = core.loader_tx.send(LoaderMsg::Thumb(page, width));
    }
    Ok(())
}

/// Fit-to-width zoom factor for the viewport (helper for the UI).
#[tauri::command]
pub fn vdf_fit_width(state: State<AppCore>, id: u64) -> Result<f64, String> {
    let core = doc(&state, id)?;
    Ok(core.fit_width())
}

#[tauri::command]
pub fn vdf_page_label(state: State<AppCore>, id: u64, page: u32) -> Result<String, String> {
    let core = doc(&state, id)?;
    Ok(format!("{} / {}", page + 1, core.page_count))
}

/// Direct tile lookup (used by the compositor right after poll reports a
/// key ready; the vdf-tile:// protocol serves the bytes).
#[tauri::command]
pub fn vdf_tile_meta(state: State<AppCore>, id: u64, key: String) -> Result<(u32, u32), String> {
    let core = doc(&state, id)?;
    let key = parse_tile_key(&key).ok_or("bad key")?;
    core.scheduler
        .tile_pixels(&key)
        .map(|(w, h, _)| (w, h))
        .ok_or_else(|| "not ready".into())
}

pub fn parse_tile_key(s: &str) -> Option<vdf_core::TileKey> {
    // "p{page}z{step}r{rot}x{x}y{y}" (see tile_key_str)
    let mut parts = s.split(|c: char| c.is_ascii_alphabetic());
    let _p = parts.next()?; // leading "p"
    let nums: Vec<u32> = parts.filter_map(|seg| seg.parse::<u32>().ok()).collect();
    if nums.len() != 5 {
        return None;
    }
    Some(vdf_core::TileKey::new(
        PageIndex(nums[0]),
        u16::try_from(nums[1]).ok()?,
        Rotation::from_quarter_turns(nums[2]),
        nums[3],
        nums[4],
    ))
}
