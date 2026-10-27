use std::num::NonZeroU8;

use campfire_common::{DeclaredName, EnumAll};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::CrowdedPlace;

/// The buttons a look places on the command panel, each at its place, the first 1, as the HUD
/// file's grid orders them (02-engine-core.md, A look places its buttons); at most one at a place.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct PanelPlaces(Vec<PanelPlace>);

/// A button at a place of the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelPlace {
    pub place: NonZeroU8,
    pub button: PanelButton,
}

/// What a panel's button does: the action of its unit's slots of the name, or a command
/// (02-engine-core.md, The panel's commands are the engine's orders).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelButton {
    Action(DeclaredName),
    Command(PanelCommand),
}

/// A command of the panel, each an order of the engine: a sale of a building, a stop, an attack
/// move to a point, and a producer's rally to a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelCommand {
    Sell,
    Stop,
    AttackMove,
    Rally,
}

impl PanelPlaces {
    /// No button at any place.
    pub const EMPTY: Self = Self(Vec::new());

    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, PanelPlace> {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a PanelPlaces {
    type Item = &'a PanelPlace;
    type IntoIter = std::slice::Iter<'a, PanelPlace>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// Places each place's button, the places in any order; a place given twice is refused, as a
/// read refuses it, so this is no collect.
impl TryFrom<Vec<PanelPlace>> for PanelPlaces {
    type Error = CrowdedPlace;

    fn try_from(mut places: Vec<PanelPlace>) -> Result<Self, CrowdedPlace> {
        places.sort_by_key(|place| place.place);
        match places
            .windows(2)
            .find(|pair| pair[0].place == pair[1].place)
        {
            Some(pair) => Err(CrowdedPlace(pair[0].place)),
            None => Ok(Self(places)),
        }
    }
}

/// A place as a file writes it: its place, and its action or its command.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaceFields {
    place: NonZeroU8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    action: Option<DeclaredName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    command: Option<PanelCommand>,
}

impl Serialize for PanelPlace {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (action, command) = match &self.button {
            PanelButton::Action(name) => (Some(name.clone()), None),
            PanelButton::Command(command) => (None, Some(*command)),
        };
        PlaceFields {
            place: self.place,
            action,
            command,
        }
        .serialize(serializer)
    }
}

/// A package's file is untrusted, so a place of neither or both of an action and a command fails
/// to read.
impl<'de> Deserialize<'de> for PanelPlace {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let PlaceFields {
            place,
            action,
            command,
        } = PlaceFields::deserialize(deserializer)?;
        let button = match (action, command) {
            (Some(action), None) => PanelButton::Action(action),
            (None, Some(command)) => PanelButton::Command(command),
            _ => {
                return Err(D::Error::custom(
                    "a panel's place holds an action or a command",
                ));
            }
        };
        Ok(Self { place, button })
    }
}

/// A package's file is untrusted, so two buttons at one place fail to read.
impl<'de> Deserialize<'de> for PanelPlaces {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let places = Vec::<PanelPlace>::deserialize(deserializer)?;
        Self::try_from(places).map_err(D::Error::custom)
    }
}
