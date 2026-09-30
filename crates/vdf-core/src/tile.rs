//! Tile keys and render generations — the data behind the no-blanking /
//! stale-render invariant (MASTER_PLAN.md §8, §10).
//!
//! A finished tile is only accepted into a cache/compositor slot when its
//! [`RenderGeneration`] is at least the generation recorded for that slot;
//! late results from superseded requests are dropped, never overwrite.

use std::fmt;

use crate::units::{PageIndex, Rotation};

/// Identity of one tile in the render cache.
///
/// `zoom_step` is an index into the quantized zoom ladder (steps of 2^(1/8)),
/// not a raw scale — continuous zoom is display-only; requests snap to the
/// ladder so tiles stay cacheable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileKey {
    pub page: PageIndex,
    pub zoom_step: u16,
    pub rotation: Rotation,
    pub x: u32,
    pub y: u32,
}

impl TileKey {
    pub fn new(page: PageIndex, zoom_step: u16, rotation: Rotation, x: u32, y: u32) -> Self {
        Self {
            page,
            zoom_step,
            rotation,
            x,
            y,
        }
    }
}

impl fmt::Display for TileKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "p{}-z{}-r{}-{}x{}",
            self.page.0,
            self.zoom_step,
            self.rotation.quarter_turns(),
            self.x,
            self.y
        )
    }
}

/// Monotonic generation counter guarding one tile slot against stale results.
///
/// The scheduler bumps a slot's generation whenever it issues a new request
/// for that slot; a result carrying an older generation is stale by definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct RenderGeneration(pub u64);

impl RenderGeneration {
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// Acceptance rule shared by every cache layer: a finished tile may only
/// occupy a slot if it is at least as new as what the slot expects.
/// `None` current generation means the slot is empty.
pub fn accepts(current: Option<RenderGeneration>, incoming: RenderGeneration) -> bool {
    match current {
        None => true,
        Some(cur) => incoming >= cur,
    }
}

/// First zoom-ladder step at or above a raw zoom factor.
/// Ladder step n has zoom 2^(n/8); step 0 = 100%.
pub fn zoom_step_for(zoom: f64) -> u16 {
    if !zoom.is_finite() || zoom <= 0.0 {
        return 0;
    }
    let n = (zoom.log2() * 8.0).ceil();
    n.clamp(i16::MIN as f64, i16::MAX as f64) as i16 as u16
}

/// Zoom factor of a ladder step.
pub fn zoom_for_step(step: u16) -> f64 {
    // interpret as signed exponent so steps > 32767 don't explode
    let s = step as i16;
    2f64.powf(s as f64 / 8.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_results_are_rejected() {
        let slot = RenderGeneration(5);
        assert!(accepts(None, RenderGeneration(1)));
        assert!(accepts(Some(slot), RenderGeneration(5)));
        assert!(accepts(Some(slot), RenderGeneration(9)));
        assert!(
            !accepts(Some(slot), RenderGeneration(4)),
            "older generation must never overwrite"
        );
    }

    #[test]
    fn generation_monotonic() {
        let g = RenderGeneration(41);
        assert_eq!(g.next(), RenderGeneration(42));
    }

    #[test]
    fn tile_key_display_stable() {
        let k = TileKey::new(PageIndex(3), u16::MAX, Rotation::D270, 12, 7);
        assert_eq!(k.to_string(), "p3-z65535-r3-12x7");
    }

    #[test]
    fn zoom_ladder_covers_and_snaps_up() {
        assert_eq!(zoom_step_for(1.0), 0);
        // 1.05 is below step 1 (≈1.0905) → snaps up to it
        assert_eq!(zoom_step_for(1.05), 1);
        assert!((zoom_for_step(1) - 1.0905).abs() < 1e-3);
        // zoom_for_step(zoom_step_for(z)) ≥ z, and within one ladder step
        for z in [0.1, 0.5, 0.99, 1.0, 1.01, 2.0, 3.7, 12.5, 64.0] {
            let snapped = zoom_for_step(zoom_step_for(z));
            assert!(snapped >= z - 1e-9, "snapped {snapped} must be >= {z}");
            assert!(
                snapped <= z * 1.091 + 1e-9,
                "snapped {snapped} too far above {z}"
            );
        }
        // non-finite input falls back to step 0
        assert_eq!(zoom_step_for(f64::NAN), 0);
        assert_eq!(zoom_step_for(0.0), 0);
    }
}
