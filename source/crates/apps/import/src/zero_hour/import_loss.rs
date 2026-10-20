use campfire_common::MapName;

use crate::zero_hour::SkippedModel;
use crate::zero_hour::combat_import::DroppedWeapon;
use crate::zero_hour::production_import::DroppedButton;
use crate::zero_hour::supply_import::DroppedSupply;

/// What the import does not hold of the install as the original holds it, each with what it is
/// and why (09-zero-hour.md, Losses).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportLoss {
    /// A model the objects name that the package lacks.
    Model(SkippedModel),
    /// A texture a model names that no archive holds, by the name the model gives, which it
    /// draws untextured.
    Texture(String),
    /// Cells of a map whose terrain draws a tile of no texture, by their count.
    UntexturedCells { map: MapName, cells: usize },
    /// A waypoint of a map past its bounds, by its name, which the import drops.
    Waypoint { map: MapName, waypoint: String },
    /// A command button the import does not convert (09-zero-hour.md, Every command set's
    /// trains and builds are imported).
    Button(DroppedButton),
    /// A supply module the import does not convert (09-zero-hour.md, A box is money).
    Supply(DroppedSupply),
    /// A weapon the import does not convert (09-zero-hour.md, A weapon converts when it hits one
    /// target at once).
    Weapon(DroppedWeapon),
    /// An object whose body the import gives no life, by its name (09-zero-hour.md, Life is the
    /// original's).
    Unliving(String),
    /// An object whose body starts below its most life, by its name, which the engine starts
    /// full.
    Wounded(String),
}
