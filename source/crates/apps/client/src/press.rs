//! The one rule of every press of a button, a selection's and the camera's: whether it drags.

use bevy::math::Vec2;

/// How far the cursor moves from where a button went down before the press drags, in logical
/// pixels along either axis: Windows' default `SM_CXDRAG` and `SM_CYDRAG`, so a hand that shakes
/// as it clicks still clicks.
const DRAG_PIXELS: f32 = 4.0;

/// Whether a press that went down at `from` drags with the cursor at `to`.
pub(crate) fn drags(from: Vec2, to: Vec2) -> bool {
    (to - from).abs().max_element() > DRAG_PIXELS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_drags_past_four_pixels_along_either_axis() {
        let from = Vec2::new(10.0, 10.0);
        assert!(!drags(from, Vec2::new(14.0, 6.0)));
        assert!(drags(from, Vec2::new(14.5, 10.0)));
        assert!(drags(from, Vec2::new(10.0, 5.5)));
    }
}
