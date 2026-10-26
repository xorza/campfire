use campfire_common::DeclaredName;
use campfire_declarations::{PatchError, Patched, Section, SectionTable};
use campfire_math::Num;
use campfire_script::{ApiVersion, Status};
use campfire_script::{DataSection, DataTable};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::turret::Turret;
use crate::unit_offset::UnitOffset;

/// A unit type's `turret` section (04-capabilities/combat.md, A unit type may have a turret):
/// its attacks of the slot kind `slot`, at the `places` among its unit's slots of that kind or
/// at every one, turn with a turret about the vertical through `pivot`, `turn_rate` degrees a
/// second, and start within `tolerance_deg` of their target's bearing; with `pitch`, the turret
/// also pitches them (04-capabilities/combat.md, A turret may pitch).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TurretData {
    pub(crate) turn_rate: Num,
    pub(crate) tolerance_deg: Num,
    pub(crate) slot: DeclaredName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) places: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "UnitOffset::is_origin")]
    pub(crate) pivot: UnitOffset,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pitch: Option<TurretPitchData>,
}

/// A turret's `pitch`: it pitches its weapons `turn_rate` degrees a second about the level axis
/// across its unit through `pivot`, to `fire_deg` when it gives one, else toward its target no
/// lower than `least_deg`, and back toward `rest_deg` when it has none, each in degrees above
/// the level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TurretPitchData {
    pub(crate) turn_rate: Num,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fire_deg: Option<Num>,
    pub(crate) least_deg: Num,
    pub(crate) rest_deg: Num,
    #[serde(skip_serializing_if = "UnitOffset::is_origin")]
    pub(crate) pivot: UnitOffset,
}

impl TurretData {
    /// A turret of `turn_rate` degrees a second and `tolerance_deg`, of the slot kind `slot`,
    /// about its unit's place, which does not pitch; `None` for a turn rate that is not positive,
    /// or a tolerance outside 0 to 180 degrees.
    pub fn new(turn_rate: Num, tolerance_deg: Num, slot: DeclaredName) -> Option<Self> {
        let held = turn_rate > Num::ZERO && (Num::ZERO..=Num::int(180)).contains(&tolerance_deg);
        held.then_some(Self {
            turn_rate,
            tolerance_deg,
            slot,
            places: None,
            pivot: UnitOffset::ORIGIN,
            pitch: None,
        })
    }

    /// The same turret, which carries the attacks of the slots at `places` among its unit's
    /// slots of its kind alone; `None` for a place past the last a turret holds,
    /// `Turret::MOST_PLACES`.
    pub fn carrying(self, places: Vec<u8>) -> Option<Self> {
        let held = places.iter().all(|&place| place < Turret::MOST_PLACES);
        held.then_some(Self {
            places: Some(places),
            ..self
        })
    }

    /// The same turret about `pivot`.
    #[must_use]
    pub fn about(self, pivot: UnitOffset) -> Self {
        Self { pivot, ..self }
    }

    /// The same turret, which pitches as `pitch` says.
    #[must_use]
    pub fn pitched(self, pitch: TurretPitchData) -> Self {
        Self {
            pitch: Some(pitch),
            ..self
        }
    }
}

impl TurretPitchData {
    /// A pitch of `turn_rate` degrees a second, from `least_deg` up toward a target, or at
    /// `fire_deg` when given, back to `rest_deg`, about its unit's place; `None` for a turn rate
    /// that is not positive, or an angle outside −90 to 90 degrees.
    pub fn new(
        turn_rate: Num,
        fire_deg: Option<Num>,
        least_deg: Num,
        rest_deg: Num,
    ) -> Option<Self> {
        let level = Num::int(-90)..=Num::int(90);
        let angles = fire_deg.iter().chain([&least_deg, &rest_deg]);
        let held = turn_rate > Num::ZERO && angles.into_iter().all(|deg| level.contains(deg));
        held.then_some(Self {
            turn_rate,
            fire_deg,
            least_deg,
            rest_deg,
            pivot: UnitOffset::ORIGIN,
        })
    }

    /// The same pitch about `pivot`.
    #[must_use]
    pub const fn about(self, pivot: UnitOffset) -> Self {
        Self { pivot, ..self }
    }
}

/// Data is untrusted, so a turn rate that is not positive, or an angle outside −90 to 90
/// degrees, fails to read; the least and the rest are level, and the pivot the unit's place, by
/// default.
impl<'de> Deserialize<'de> for TurretPitchData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            turn_rate: Num,
            fire_deg: Option<Num>,
            #[serde(default)]
            least_deg: Num,
            #[serde(default)]
            rest_deg: Num,
            #[serde(default)]
            pivot: UnitOffset,
        }
        let fields = Fields::deserialize(deserializer)?;
        let pitch = Self::new(
            fields.turn_rate,
            fields.fire_deg,
            fields.least_deg,
            fields.rest_deg,
        );
        let pitch = pitch.ok_or_else(|| {
            D::Error::custom(
                "a turret's pitch turns at a positive rate, its angles from -90 to 90 degrees",
            )
        })?;
        Ok(pitch.about(fields.pivot))
    }
}

/// A unit type's `turret` as a tool writes it in part, over the defaults it lies on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurretPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_rate: Option<Num>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tolerance_deg: Option<Num>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<DeclaredName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub places: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pivot: Option<UnitOffset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pitch: Option<TurretPitchData>,
}

/// Data is untrusted, so a turn rate that is not positive, a tolerance outside 0 to 180 degrees,
/// or a place past the last a turret holds, fails to read.
impl Patched for TurretData {
    type Patch = TurretPatch;

    fn from_patch(patch: TurretPatch) -> Result<Self, PatchError> {
        let turn_rate = patch.turn_rate.ok_or(PatchError::Missing("turn_rate"))?;
        let tolerance_deg = patch
            .tolerance_deg
            .ok_or(PatchError::Missing("tolerance_deg"))?;
        let slot = patch.slot.ok_or(PatchError::Missing("slot"))?;
        let turret = Self::new(turn_rate, tolerance_deg, slot).ok_or(PatchError::Refused(
            "a turret's turn rate is positive, and its tolerance from 0 to 180 degrees",
        ))?;
        let turret = match patch.places {
            Some(places) => turret.carrying(places).ok_or(PatchError::Refused(
                "a turret's places are among its unit's first 64 slots of its kind",
            ))?,
            None => turret,
        };
        let turret = turret.about(patch.pivot.unwrap_or_default());
        Ok(match patch.pitch {
            Some(pitch) => turret.pitched(pitch),
            None => turret,
        })
    }
}

impl<'de> Deserialize<'de> for TurretData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::read_patch(deserializer)
    }
}

impl DataSection for TurretData {
    const TABLE: DataTable = DataTable::new("A unit type's `turret`");
}

impl Section for TurretData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "turret";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
