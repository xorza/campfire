use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::panel_icon::PanelIcon;

/// The panel's power bar (power.md, The panel shows the power bar): the base of the logarithm its
/// lengths take, the intervals of it its width holds, the margin of use under the power given
/// within which it turns yellow, the fill of each state, and the needle of the power used, as
/// `W3DPowerDraw` draws Zero Hour's of `PowerBarBase`, `PowerBarIntervals` and
/// `PowerBarYellowRange`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PowerBar {
    base: u8,
    intervals: Num,
    margin: u32,
    fills: PowerFills,
    needle: PanelIcon,
}

/// The tiles a power bar's fill repeats: of power enough with room, of power within the margin,
/// and of power short.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerFills {
    pub enough: PanelIcon,
    pub near: PanelIcon,
    pub short: PanelIcon,
}

impl PowerBar {
    /// The bar of `base`, at least 2, `intervals`, above 0, `margin`, `fills` and `needle`; none
    /// for a base or intervals outside them.
    pub fn new(
        base: u8,
        intervals: Num,
        margin: u32,
        fills: PowerFills,
        needle: PanelIcon,
    ) -> Option<Self> {
        (base >= 2 && intervals > Num::ZERO).then_some(Self {
            base,
            intervals,
            margin,
            fills,
            needle,
        })
    }

    pub const fn fills(&self) -> &PowerFills {
        &self.fills
    }

    pub const fn needle(&self) -> &PanelIcon {
        &self.needle
    }

    /// The base of the logarithm its lengths take.
    pub const fn base(&self) -> u8 {
        self.base
    }

    /// The intervals of the logarithm its width holds.
    pub const fn intervals(&self) -> Num {
        self.intervals
    }

    /// The margin of use under the power given within which it shows its fill `near`.
    pub const fn margin(&self) -> u32 {
        self.margin
    }
}

/// A client file is untrusted, so a base below 2 or intervals of none fail to read.
impl<'de> Deserialize<'de> for PowerBar {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            base: u8,
            intervals: Num,
            margin: u32,
            fills: PowerFills,
            needle: PanelIcon,
        }
        let Fields {
            base,
            intervals,
            margin,
            fills,
            needle,
        } = Fields::deserialize(deserializer)?;
        Self::new(base, intervals, margin, fills, needle).ok_or_else(|| {
            D::Error::custom("a power bar's base is at least 2 and its intervals above 0")
        })
    }
}

#[cfg(test)]
mod tests;
