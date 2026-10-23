use campfire_common::MapName;

use crate::zero_hour::dropped::Dropped;
use crate::zero_hour::format::ini::object_ini::build_variations::VariationDropCause;
use crate::zero_hour::model_writer::SkippedModel;
use crate::zero_hour::production_import::ButtonDropCause;
use crate::zero_hour::supply_import::SupplyDropCause;
use crate::zero_hour::weapon_drop_cause::WeaponDropCause;

/// What the import does not hold of the install as the original holds it, each with what it is
/// and why (09-zero-hour.md, Losses).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportLoss {
    /// A model the objects name that the package lacks.
    Model(SkippedModel),
    /// A texture a model names that no archive holds, by the name the model gives, which it
    /// draws as the game's `MissingTexture`, its lit color tinted.
    Texture(String),
    /// Cells of a map whose terrain draws a tile of no texture, by their count.
    UntexturedCells { map: MapName, cells: usize },
    /// A waypoint of a map past its bounds, by its name, which the import drops.
    Waypoint { map: MapName, waypoint: String },
    /// A command button the import does not convert (09-zero-hour.md, Every command set's
    /// trains and builds are imported).
    Button(Dropped<ButtonDropCause>),
    /// A supply module the import does not convert (09-zero-hour.md, A box is money).
    Supply(Dropped<SupplyDropCause>),
    /// A weapon the import does not convert (09-zero-hour.md, A weapon converts when it hits one
    /// target at once).
    Weapon(Dropped<WeaponDropCause>),
    /// An object whose body the import gives no life, by its name (09-zero-hour.md, Life is the
    /// original's).
    Unliving(String),
    /// An object whose body starts below its most life, by its name, which the engine starts
    /// full.
    Wounded(String),
    /// An object's build variations the import converts otherwise than whole, by the parent's
    /// name, or a map's placement of a variation it converts otherwise than to its parent, by the
    /// variation's (09-zero-hour.md, A chassis converts to the parent of its file).
    Variation(Dropped<VariationDropCause>),
}
