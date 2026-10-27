use serde::{Deserialize, Serialize};

/// How a tread lays its track (wrecks.md, A tread lays track marks): an edge each `spacing`
/// metres it moves, its ends `lift` metres above the drawn ground, up to `edges` edges, past
/// which the oldest goes; the oldest edges past the newest `opaque` fade by their place, and each
/// fades over `fade` seconds; at most `tracks` tracks at once, a unit past them laying none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackMarks {
    pub spacing: f32,
    pub lift: f32,
    pub edges: u16,
    pub opaque: u16,
    pub fade: f32,
    pub tracks: u16,
}

impl TrackMarks {
    /// No tread lays a track.
    pub const NONE: Self = Self {
        spacing: 0.0,
        lift: 0.0,
        edges: 0,
        opaque: 0,
        fade: 0.0,
        tracks: 0,
    };

    /// Whether it holds: none, or a spacing and a fade above 0, a finite lift, at least two edges,
    /// of which at most all opaque.
    pub fn holds(&self) -> bool {
        let above = |value: f32| value > 0.0 && value.is_finite();
        self.is_none()
            || (above(self.spacing)
                && above(self.fade)
                && self.lift.is_finite()
                && self.edges >= 2
                && self.opaque <= self.edges)
    }

    pub fn is_none(&self) -> bool {
        *self == Self::NONE
    }
}
