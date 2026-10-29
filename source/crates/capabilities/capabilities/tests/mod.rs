//! The tests that build a whole match, which `release` composes here: one module for each
//! capability crate, and one for each of the core's crates.

use campfire_capabilities::Match;
use campfire_combat::internals::Armed;
use campfire_math::{Num, Vec3};
use campfire_values::{Share, UnitType};
use serde::Deserialize;
use serde::de::{IntoDeserializer, value};

mod abilities;
mod areas;
mod combat;
mod interaction;
mod items;
mod navigation;
mod orders;
mod players;
mod production;
mod progression;
mod projectiles;
mod scripts;
mod stats;
mod units;
mod vision;

/// The point `(x, z)` in halves of a meter.
fn halves(x: i64, z: i64) -> Vec3 {
    Vec3::new(Num::HALF * x, Num::ZERO, Num::HALF * z)
}

/// The share `text` gives, as data reads it.
fn share(text: &str) -> Share {
    Share::deserialize(IntoDeserializer::<value::Error>::into_deserializer(text)).unwrap()
}

/// `health`, and `damage` within `range`, a windup and a period in ticks.
const fn combatant(health: i64, range: i64, windup: u64, period: u64, damage: i64) -> Armed {
    Armed::melee(
        Num::int(health),
        Num::int(range),
        windup,
        period,
        Num::int(damage),
    )
}

/// Gives `unit_type` the combat `armed`.
fn arm(game: &mut Match, unit_type: UnitType, armed: Armed) -> UnitType {
    armed.give(game, unit_type);
    unit_type
}
