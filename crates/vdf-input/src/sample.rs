//! One pointer/stylus sample with the full optional hardware channel set.

/// Which pointer produced a sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Pen,
    Eraser,
    Mouse,
    Touch,
    Unknown,
}

/// Button bitset for a sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PointerButtons(pub u8);

impl PointerButtons {
    pub const LEFT: u8 = 1 << 0;
    pub const RIGHT: u8 = 1 << 1;
    /// Stylus barrel / side button.
    pub const BARREL: u8 = 1 << 2;

    pub fn has(self, bit: u8) -> bool {
        self.0 & bit != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Pressure substituted for devices that do not report it (mouse, some
/// touch), so the brush engine always has a defined response curve.
pub const SYNTHETIC_PRESSURE: f32 = 0.5;

/// A single pointer/stylus sample in device coordinates (conversion to
/// document space happens in the pipeline, not here).
///
/// Optional channels are `None` when the device does not report them —
/// graceful degradation is explicit, not silent zero.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerSample {
    pub pointer_id: u32,
    pub device_id: u32,
    pub tool: Tool,
    pub x: f32,
    pub y: f32,
    /// Normalized 0..1 when reported.
    pub pressure: Option<f32>,
    pub tilt_x: Option<f32>,
    pub tilt_y: Option<f32>,
    pub azimuth: Option<f32>,
    pub altitude: Option<f32>,
    /// Monotonic timestamp in milliseconds.
    pub t_ms: f64,
    pub buttons: PointerButtons,
    pub contact: bool,
    pub hover: bool,
}

impl PointerSample {
    /// Pressure for brush math: the reported value when present and in
    /// range, otherwise the synthetic constant.
    pub fn effective_pressure(&self) -> f32 {
        match self.pressure {
            Some(p) if (0.0..=1.0).contains(&p) => p,
            _ => SYNTHETIC_PRESSURE,
        }
    }

    /// A minimal mouse-style sample (M0 plumbing/tests).
    pub fn mouse(pointer_id: u32, x: f32, y: f32, t_ms: f64) -> Self {
        Self {
            pointer_id,
            device_id: 0,
            tool: Tool::Mouse,
            x,
            y,
            pressure: None,
            tilt_x: None,
            tilt_y: None,
            azimuth: None,
            altitude: None,
            t_ms,
            buttons: PointerButtons(PointerButtons::LEFT),
            contact: true,
            hover: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_degrades_gracefully() {
        let mouse = PointerSample::mouse(1, 10.0, 20.0, 0.0);
        assert_eq!(mouse.effective_pressure(), SYNTHETIC_PRESSURE);

        let mut pen = mouse;
        pen.tool = Tool::Pen;
        pen.pressure = Some(0.75);
        assert_eq!(pen.effective_pressure(), 0.75);

        // out-of-range pressure falls back rather than corrupting the stroke
        pen.pressure = Some(1.5);
        assert_eq!(pen.effective_pressure(), SYNTHETIC_PRESSURE);
        pen.pressure = Some(f32::NAN);
        assert_eq!(pen.effective_pressure(), SYNTHETIC_PRESSURE);
    }

    #[test]
    fn buttons_bitset() {
        let b = PointerButtons(PointerButtons::LEFT | PointerButtons::BARREL);
        assert!(b.has(PointerButtons::BARREL));
        assert!(!b.has(PointerButtons::RIGHT));
        assert!(!b.is_empty());
    }
}
