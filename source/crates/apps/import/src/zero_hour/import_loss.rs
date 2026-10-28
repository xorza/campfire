use std::fmt::{self, Display, Formatter};

use campfire_common::MapName;
use serde::{Deserialize, Serialize};

use crate::zero_hour::animation_import::AnimationDropCause;
use crate::zero_hour::contain_drop_cause::ContainDropCause;
use crate::zero_hour::dropped::Dropped;
use crate::zero_hour::dropped_roads::DroppedRoads;
use crate::zero_hour::effect_drop_cause::EffectDropCause;
use crate::zero_hour::format::ini::object_ini::build_variations::VariationDropCause;
use crate::zero_hour::model_writer::{DroppedStates, SkipReason, SkippedModel};
use crate::zero_hour::panel_drop_cause::PanelDropCause;
use crate::zero_hour::power_drop_cause::PowerDropCause;
use crate::zero_hour::production_import::ButtonDropCause;
use crate::zero_hour::sound_drop_cause::SoundDropCause;
use crate::zero_hour::supply_import::SupplyDropCause;
use crate::zero_hour::upgrade_import::UpgradeDropCause;
use crate::zero_hour::weapon_drop_cause::WeaponDropCause;

/// What the import does not hold of the install as the original holds it, each with what it is
/// and why (09-zero-hour.md, Losses).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportLoss {
    /// A model the objects name that the package lacks.
    Model(SkippedModel),
    /// An animation a look names that the package lacks (09-zero-hour.md, An animation is a
    /// glTF file of its own).
    Animation(Dropped<AnimationDropCause>),
    /// A texture a model names that no archive holds, by the name the model gives, which it
    /// draws as the game's `MissingTexture`, its lit color tinted.
    Texture(String),
    /// Cells of a map whose terrain draws a tile of no texture, by their count.
    UntexturedCells { map: MapName, cells: usize },
    /// A waypoint of a map past its bounds, by its name, which the import drops.
    Waypoint { map: MapName, waypoint: String },
    /// A map's road segments of a type the import does not draw (09-zero-hour.md, A road's type is
    /// `Roads.ini`'s).
    Road { map: MapName, dropped: DroppedRoads },
    /// A command button the import does not convert (09-zero-hour.md, Every command set's
    /// trains and builds are imported).
    Button(Dropped<ButtonDropCause>),
    /// A supply module the import does not convert (09-zero-hour.md, A box is money).
    Supply(Dropped<SupplyDropCause>),
    /// An upgrade module, or an object's weapon sets and armor sets, the import does not
    /// convert (09-zero-hour.md, Zero Hour's upgrade flags are tags and stats).
    Upgrade(Dropped<UpgradeDropCause>),
    /// A weapon the import does not convert (09-zero-hour.md, A weapon converts when it hits one
    /// target at once).
    Weapon(Dropped<WeaponDropCause>),
    /// An object whose body the import gives no life, by its name (09-zero-hour.md, Life is the
    /// original's).
    Unliving(String),
    /// An object whose body starts below its most life, by its name, which the engine starts
    /// full.
    Wounded(String),
    /// A weapon whose `PreferredAgainst` names a kind of no tag, by its name, which prefers no
    /// target (09-zero-hour.md, The import converts what the choice and the answer read).
    Unpreferred(String),
    /// A trainable object whose `ExperienceRequired` does not start at 0 and rise in each step,
    /// by its name, which gains no level (09-zero-hour.md, Veterancy is progression's data).
    Untrainable(String),
    /// An object's build variations the import converts otherwise than whole, by the parent's
    /// name, or a map's placement of a variation it converts otherwise than to its parent, by the
    /// variation's (09-zero-hour.md, A chassis converts to the parent of its file).
    Variation(Dropped<VariationDropCause>),
    /// The condition states of an object's draw module, or of its whole look, the import does not
    /// convert (09-zero-hour.md, A look's draws are its draw modules).
    States(DroppedStates),
    /// A particle system, a part of one, or a nugget of an effect list, by the system's or the
    /// list's name, the import does not convert (09-zero-hour.md, Particles and effects).
    Effect(Dropped<EffectDropCause>),
    /// A command set's button, or a part of one, the panel does not show (09-zero-hour.md, The
    /// panel).
    Panel(Dropped<PanelDropCause>),
    /// A special power's button, by its name, the import converts to no cast (09-zero-hour.md, A
    /// power is a cast of its building).
    Power(Dropped<PowerDropCause>),
    /// An audio event, by its name, or a part of how the game plays it, the import does not
    /// convert (09-zero-hour.md, Sound).
    Sound(Dropped<SoundDropCause>),
    /// An object's contain module, or a part of one, by the object's name, the import does not
    /// convert (09-zero-hour.md, Transports and garrisons).
    Contain(Dropped<ContainDropCause>),
}

/// What the loss is, then the part of the install it is of, and why.
impl Display for ImportLoss {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Model(skipped) => {
                let what = match skipped.reason {
                    SkipReason::Missing => "an object names a model no file of the install holds",
                    SkipReason::Emitter => {
                        "an object names a particle emitter, which the importer does not convert"
                    }
                    SkipReason::Aggregate => {
                        "a model's HLOD adds an aggregate, which the importer does not convert"
                    }
                };
                write!(f, "{what}: {}", skipped.name)
            }
            Self::Animation(animation) => {
                write!(f, "a look's animation is not imported: {animation}")
            }
            Self::Texture(texture) => write!(
                f,
                "a model names a texture no archive holds, and draws the game's MissingTexture: \
                 {texture}"
            ),
            Self::UntexturedCells { map, cells } => write!(
                f,
                "a map's terrain draws cells of a tile with no texture: {map}, {cells} cells"
            ),
            Self::Waypoint { map, waypoint } => write!(
                f,
                "a map's waypoint past its bounds is dropped: {map}, {waypoint}"
            ),
            Self::Road { map, dropped } => {
                write!(f, "a map's road segments are not drawn: {map}, {dropped}")
            }
            Self::Button(button) => {
                write!(f, "a command button is not imported: {button}")
            }
            Self::Supply(supply) => {
                write!(f, "an object's supply is not imported: {supply}")
            }
            Self::Upgrade(module) => {
                write!(
                    f,
                    "an upgrade module, or an object's sets, are not imported: {module}"
                )
            }
            Self::Weapon(weapon) => {
                write!(f, "a weapon is not imported: {weapon}")
            }
            Self::Unliving(object) => {
                write!(
                    f,
                    "an object's body is none the import gives life: {object}"
                )
            }
            Self::Wounded(object) => write!(
                f,
                "an object's body starts below its most life, which the engine starts full: \
                 {object}"
            ),
            Self::Unpreferred(weapon) => write!(
                f,
                "a weapon prefers a kind of no tag, which no unit is: {weapon}"
            ),
            Self::Untrainable(object) => write!(
                f,
                "a trainable object's experience levels do not rise from 0, so it gains no \
                 level: {object}"
            ),
            Self::Variation(variation) => {
                write!(
                    f,
                    "an object's build variations are not imported whole: {variation}"
                )
            }
            Self::States(states) => write!(
                f,
                "a look's condition states are not imported: {} ({:?}), {} states",
                states.object, states.cause, states.states
            ),
            Self::Effect(effect) => write!(f, "an effect is not imported: {effect}"),
            Self::Panel(button) => write!(f, "a panel's button is not shown whole: {button}"),
            Self::Power(button) => write!(f, "a special power is not a cast: {button}"),
            Self::Sound(event) => write!(f, "a sound is not played whole: {event}"),
            Self::Contain(contain) => {
                write!(
                    f,
                    "an object's contain module is not imported whole: {contain}"
                )
            }
        }
    }
}
