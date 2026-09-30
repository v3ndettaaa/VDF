//! Newtyped units and view-state scalars.

/// Length in PDF user-space units (1/72 inch), y-up.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct PdfPoints(pub f64);

/// Length in physical device pixels (already includes device pixel ratio).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct DevicePixels(pub u32);

/// Length in CSS pixels (device px ÷ devicePixelRatio).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct CssPixels(pub f64);

/// Position of a page in the document's page sequence (0-based).
/// This is a *sequence* index, not identity — [`crate::PageId`] is identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PageIndex(pub u32);

/// Clamped zoom factor. 1.0 = 100%.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct ZoomFactor(f64);

impl ZoomFactor {
    pub const MIN: f64 = 0.05;
    pub const MAX: f64 = 64.0;

    pub fn new(z: f64) -> Self {
        if !z.is_finite() {
            return Self(1.0);
        }
        Self(z.clamp(Self::MIN, Self::MAX))
    }

    pub fn get(self) -> f64 {
        self.0
    }

    pub fn percent(self) -> f64 {
        self.0 * 100.0
    }
}

impl Default for ZoomFactor {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Page rotation in quarter turns clockwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Rotation {
    #[default]
    D0,
    D90,
    D180,
    D270,
}

impl Rotation {
    pub fn from_quarter_turns(q: u32) -> Self {
        match q % 4 {
            0 => Rotation::D0,
            1 => Rotation::D90,
            2 => Rotation::D180,
            _ => Rotation::D270,
        }
    }

    pub fn quarter_turns(self) -> u32 {
        match self {
            Rotation::D0 => 0,
            Rotation::D90 => 1,
            Rotation::D180 => 2,
            Rotation::D270 => 3,
        }
    }

    pub fn radians(self) -> f64 {
        self.quarter_turns() as f64 * std::f64::consts::FRAC_PI_2
    }

    pub fn next_clockwise(self) -> Self {
        Self::from_quarter_turns(self.quarter_turns() + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_is_clamped() {
        assert_eq!(ZoomFactor::new(0.0).get(), ZoomFactor::MIN);
        assert_eq!(ZoomFactor::new(1000.0).get(), ZoomFactor::MAX);
        assert_eq!(ZoomFactor::new(f64::NAN).get(), 1.0);
        assert_eq!(ZoomFactor::new(1.5).percent(), 150.0);
    }

    #[test]
    fn rotation_wraps() {
        assert_eq!(Rotation::from_quarter_turns(4), Rotation::D0);
        assert_eq!(Rotation::from_quarter_turns(5), Rotation::D90);
        assert_eq!(Rotation::D270.next_clockwise(), Rotation::D0);
        assert_eq!(Rotation::D90.radians(), std::f64::consts::FRAC_PI_2);
    }
}
