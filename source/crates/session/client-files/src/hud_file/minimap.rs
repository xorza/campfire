use std::num::{NonZeroU8, NonZeroU16};

use campfire_common::DeclaredName;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// The minimap's look (02-engine-core.md, A minimap by a radar tag; 02-engine-core.md, The minimap
/// is drawn from the terrain at load): the tag of a unit that gives its player a radar, none for a
/// minimap at all times; its cells across and down, over the map; the share of its light, in
/// percent, its terrain loses from the map's highest ground to its lowest; and the side of a unit's
/// square, in cells. A mode of none shows 128 cells at all times, darkened by 60 % at the lowest,
/// with squares of 2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Minimap {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radar: Option<DeclaredName>,
    pub cells: NonZeroU16,
    pub dark: u8,
    pub dot: NonZeroU8,
}

impl Default for Minimap {
    fn default() -> Self {
        Self {
            radar: None,
            cells: NonZeroU16::new(128).expect("128 is no zero"),
            dark: 60,
            dot: NonZeroU8::new(2).expect("2 is no zero"),
        }
    }
}

/// A darkening past 100 %, or a square wider than the minimap, fails to read.
impl<'de> Deserialize<'de> for Minimap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        struct Fields {
            radar: Option<DeclaredName>,
            cells: NonZeroU16,
            dark: u8,
            dot: NonZeroU8,
        }
        impl Default for Fields {
            fn default() -> Self {
                let Minimap {
                    radar,
                    cells,
                    dark,
                    dot,
                } = Minimap::default();
                Self {
                    radar,
                    cells,
                    dark,
                    dot,
                }
            }
        }
        let Fields {
            radar,
            cells,
            dark,
            dot,
        } = Fields::deserialize(deserializer)?;
        if dark > 100 {
            return Err(D::Error::custom("a minimap darkens by at most 100 %"));
        }
        if u16::from(dot.get()) > cells.get() {
            return Err(D::Error::custom("a minimap's square is at most its cells"));
        }
        Ok(Self {
            radar,
            cells,
            dark,
            dot,
        })
    }
}
