//! ⌘+ and ⌘−: the whole window drawn larger or smaller, as a browser
//! zooms a page.
//!
//! The interface is sized in fixed pixels throughout, so rather than
//! threading a scale through every view, this module's [`px`] stands in for
//! gpui's: every length the views give goes through it and comes out
//! multiplied by the zoom. A mouse position comes back from the toolkit in
//! real pixels, so one that is kept and drawn again later (a menu opened
//! where the click was, a dock dragged to the pointer) goes through
//! [`unzoom`] first.

use std::sync::atomic::{AtomicU32, Ordering};

use gpui::Pixels;

/// The zoom as `f32` bits: read on every length drawn, so an atomic
/// rather than state threaded through.
static ZOOM: AtomicU32 = AtomicU32::new(0x3f80_0000); // 1.0

pub const MIN: f32 = 0.7;
pub const MAX: f32 = 2.0;
const STEP: f32 = 0.1;

pub fn get() -> f32 {
    f32::from_bits(ZOOM.load(Ordering::Relaxed))
}

/// Sets the zoom, kept within [`MIN`]–[`MAX`] and to a tenth, so stepping
/// up and back down lands where it started. Returns what it was set to.
pub fn set(zoom: f32) -> f32 {
    let zoom = if zoom.is_finite() { zoom } else { 1.0 };
    let zoom = ((zoom * 10.).round() / 10.).clamp(MIN, MAX);
    ZOOM.store(zoom.to_bits(), Ordering::Relaxed);
    zoom
}

/// One step in (`by` 1) or out (`by` -1).
pub fn step(by: i32) -> f32 {
    set(get() + by as f32 * STEP)
}

/// A length in the interface's own pixels, drawn at the zoom.
pub fn px(value: f32) -> Pixels {
    gpui::px(value * get())
}

/// A position or length the toolkit measured, back in the interface's own
/// pixels.
pub fn unzoom(value: Pixels) -> f32 {
    f32::from(value) / get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_round_trip_and_stay_in_range() {
        set(1.0);
        assert_eq!(step(1), 1.1);
        assert_eq!(step(-1), 1.0);
        assert_eq!(set(5.0), MAX);
        assert_eq!(set(0.1), MIN);
        assert_eq!(set(f32::NAN), 1.0);
        set(1.0);
    }
}
