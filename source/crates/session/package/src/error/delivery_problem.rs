use campfire_common::DeclaredName;
use thiserror::Error;

use crate::error::place::Place;

/// What is wrong with a projectile or an area type, or with what delivers or makes one.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeliveryProblem {
    /// A homing projectile type at `at` is no faster than the move speed cap, so it might never
    /// catch its target.
    #[error("{0}: homing projectile no faster than the move speed cap")]
    NotFaster(Place),
    /// A unit type at `at` with a `projectile` or an `area` section has both, or a section of a
    /// unit that stands but `vision`; a dependency's unit type is no delivery type; or an avatar is
    /// one.
    #[error(
        "{0}: a projectile or area type has tags, params, one of the two sections and a \
         vision section alone, a dependency's unit types are delivery types, and an avatar is \
         none"
    )]
    NotDelivery(Place),
    /// An action's `delivery` names a unit type of its package with no section of its kind.
    #[error(
        "action \"{action}\" delivers unit type \"{unit_type}\", which has no section of its \
         delivery's kind"
    )]
    WrongSection {
        action: DeclaredName,
        unit_type: DeclaredName,
    },
    /// An action that aims at nothing delivers a projectile, which has no way to fly.
    #[error("action \"{0}\" aims at nothing, and a projectile needs an aim")]
    NoAim(DeclaredName),
    /// An action's projectile homes, and the action aims at no unit, or launches more than one.
    #[error("action \"{0}\": a homing projectile flies one at a time, at a unit target")]
    Homing(DeclaredName),
    /// An action's projectile flies a curve, and the action aims along a direction, or launches
    /// more than one.
    #[error("action \"{0}\": a curve flies one at a time, at a unit or a point")]
    Curve(DeclaredName),
    /// An action that aims along a direction delivers an area, which lands on a point.
    #[error(
        "action \"{0}\": an area lands on a point, a unit or the caster, not along a \
         direction"
    )]
    AreaDirection(DeclaredName),
    /// A weapon's splash has no projectile to go off where it ends.
    #[error("weapon \"{0}\": a splash goes off where its delivery's projectile ends")]
    Splash(DeclaredName),
    /// A weapon's scatter has no curve to aim at a point.
    #[error("weapon \"{0}\": a scatter aims its delivery's curve at a point")]
    Scatter(DeclaredName),
    /// A weapon of a delivery has a speed, which only a weapon of none takes: its delivery flies
    /// at its own speed.
    #[error("weapon \"{0}\": a speed delays the hit of a weapon of no delivery")]
    Speed(DeclaredName),
    /// A weapon's delivery is neither a homing projectile nor a curve.
    #[error("weapon \"{0}\": its delivery is a homing projectile or a curve")]
    Weapon(DeclaredName),
    /// A train makes a projectile or an area type, whose units only actions deliver.
    #[error("train \"{0}\" makes a projectile or an area type")]
    Trained(DeclaredName),
    /// A projectile type at `at` hits nothing, beside a width, a stop at its first hit, a hit once
    /// a cast or homing, which only hits use.
    #[error(
        "{0}: a projectile that hits nothing has no width, no stop on hit, no hit once a \
         cast, and does not home"
    )]
    HitsNothing(Place),
    /// An action has an `on_hit` list or hook, and its projectile hits nothing.
    #[error("action \"{0}\" has an `on_hit`, and its projectile hits nothing")]
    NoHit(DeclaredName),
}
