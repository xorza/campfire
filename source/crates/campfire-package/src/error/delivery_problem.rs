use std::fmt;

use campfire_capabilities::DeclaredName;

use crate::error::place::Place;

/// What is wrong with a projectile or an area type, or with what delivers or makes one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryProblem {
    /// A homing projectile type at `at` is no faster than the move speed cap, so it might never
    /// catch its target.
    NotFaster(Place),
    /// A unit type at `at` with a `projectile` or an `area` section has both, or a section of a
    /// unit that stands but `vision`; a dependency's unit type is no delivery type; or an avatar is
    /// one.
    NotDelivery(Place),
    /// An action's `delivery` names a unit type of its package with no section of its kind.
    WrongSection {
        action: DeclaredName,
        unit_type: DeclaredName,
    },
    /// An action that aims at nothing delivers a projectile, which has no way to fly.
    NoAim(DeclaredName),
    /// An action's projectile homes, and the action aims at no unit, or launches more than one.
    Homing(DeclaredName),
    /// An action that aims along a direction delivers an area, which lands on a point.
    AreaDirection(DeclaredName),
    /// A weapon's delivery is no homing projectile.
    Weapon(DeclaredName),
    /// An area type has a time that does not count in ticks at the fastest rate the mode allows.
    AreaTime(DeclaredName),
    /// A train makes a projectile or an area type, whose units only actions deliver.
    Trained(DeclaredName),
    /// A projectile type at `at` hits nothing, beside a width, a stop at its first hit, a hit once
    /// a cast or homing, which only hits use.
    HitsNothing(Place),
    /// An action has an `on_hit` list or hook, and its projectile hits nothing.
    NoHit(DeclaredName),
}

impl fmt::Display for DeliveryProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeliveryProblem::NotFaster(at) => {
                write!(
                    f,
                    "{at}: homing projectile no faster than the move speed cap"
                )
            }
            DeliveryProblem::NotDelivery(at) => write!(
                f,
                "{at}: a projectile or area type has tags, params, one of the two sections and a \
                 vision section alone, a dependency's unit types are delivery types, and an \
                 avatar is none"
            ),
            DeliveryProblem::WrongSection { action, unit_type } => write!(
                f,
                "action \"{action}\" delivers unit type \"{unit_type}\", which has no section of \
                 its delivery's kind"
            ),
            DeliveryProblem::NoAim(action) => {
                write!(
                    f,
                    "action \"{action}\" aims at nothing, and a projectile needs an aim"
                )
            }
            DeliveryProblem::Homing(action) => write!(
                f,
                "action \"{action}\": a homing projectile flies one at a time, at a unit target"
            ),
            DeliveryProblem::AreaDirection(action) => write!(
                f,
                "action \"{action}\": an area lands on a point, a unit or the caster, not along a \
                 direction"
            ),
            DeliveryProblem::Weapon(action) => {
                write!(
                    f,
                    "weapon \"{action}\": its delivery is a homing projectile"
                )
            }
            DeliveryProblem::Trained(action) => {
                write!(f, "train \"{action}\" makes a projectile or an area type")
            }
            DeliveryProblem::HitsNothing(at) => write!(
                f,
                "{at}: a projectile that hits nothing has no width, no stop on hit, no hit once a \
                 cast, and does not home"
            ),
            DeliveryProblem::NoHit(action) => write!(
                f,
                "action \"{action}\" has an `on_hit`, and its projectile hits nothing"
            ),
            DeliveryProblem::AreaTime(unit_type) => {
                write!(
                    f,
                    "unit type {unit_type}: a time too large to count in ticks"
                )
            }
        }
    }
}
