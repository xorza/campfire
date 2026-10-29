use std::ops::ControlFlow;

use campfire_geometry::{BodyGrid, Indexed, Placed, Radius, Shape};
use campfire_math::{Direction, Num, Vec3};
use campfire_sim::StableId;
use campfire_units::Layer;
use campfire_values::Team;

/// The units that may block a walker, as the Move stage starts, by stable id, and a grid of their
/// bodies: what each walker's brake reads, so the order the walkers move in changes none of them
/// (entries-and-blocking.md, A walker brakes behind a unit that blocks it). Kept between ticks, so
/// a tick allocates nothing once its buffers have grown.
#[derive(Debug, Default)]
pub(crate) struct Blocking {
    units: Vec<Blocker>,
    grid: BodyGrid<Layer, u32>,
}

/// A unit as a walker's brake reads it: its body where it stands, its team, if any, the way it
/// walks this tick, none for one that stands, the step it is to take that way, as the brake of the
/// tick before let it or its speed is, whether it leaves its producer or gathers, as such a unit
/// passes through some others, its weight, and the unit its walk goes into, if any, which it
/// passes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Blocker {
    pub(crate) id: StableId,
    pub(crate) position: Vec3,
    pub(crate) radius: Num,
    pub(crate) layer: Layer,
    pub(crate) team: Option<Team>,
    pub(crate) way: Option<Direction<2>>,
    pub(crate) pace: Num,
    pub(crate) leaving: bool,
    pub(crate) gathering: bool,
    pub(crate) weight: Num,
    pub(crate) into: Option<StableId>,
}

/// The way a walker goes this tick, and how far it goes that way before it turns at a waypoint,
/// none for a mover, which turns as it drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Going {
    pub(crate) way: Direction<2>,
    pub(crate) leg: Option<Num>,
}

/// What blocks a walker this tick: the longest step it may take, none when nothing blocks it, and,
/// when it steps aside for one that blocks it head on, how far: the two radii.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Brake {
    pub(crate) step: Option<Num>,
    pub(crate) aside: Option<Num>,
}

/// How a unit lies along a walker's way: how far ahead, and how far to the side, in bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Bearing {
    along: i128,
    apart: i128,
}

impl Bearing {
    /// How the offset `off`, in bits along x and z, lies along `way`.
    pub(crate) fn of(way: &Direction<2>, off: [i128; 2]) -> Self {
        Self {
            along: way.along(off),
            apart: way.apart(off),
        }
    }

    /// Whether it lies ahead within 45° of the way, the cone in which a unit that walks blocks,
    /// as Zero Hour's `blockedBy` takes it, both bounds in.
    pub(crate) const fn in_cone(self) -> bool {
        self.along > 0 && self.apart * self.apart <= self.along * self.along
    }

    /// Whether it lies ahead within the narrower cone in which a unit that stands blocks: Zero
    /// Hour's three quarters of 45°, 33.75°, as the cone of tangent 2/3, 33.69°, which the
    /// measures test exactly.
    const fn in_narrow_cone(self) -> bool {
        self.along > 0 && 9 * self.apart * self.apart <= 4 * self.along * self.along
    }
}

impl Blocking {
    /// Reads `units`, those that may block a walker this tick.
    pub(crate) fn read(&mut self, units: impl IntoIterator<Item = Blocker>) {
        self.units.clear();
        self.units.extend(units);
        self.units.sort_unstable_by_key(|unit| unit.id);
        let units = &self.units;
        self.grid
            .rebuild(units.iter().enumerate().filter_map(|(at, unit)| {
                Some(Indexed {
                    id: u32::try_from(at).expect("a tick's units fit a u32"),
                    key: unit.layer,
                    body: Placed::new(unit.position, Shape::Circle(Radius::new(unit.radius)?)),
                })
            }));
    }

    /// The brake of the walker `id`, which goes `way` a step of `step` this tick. A unit blocks it
    /// when the walker's body, going that way, would reach the unit's within the step and before
    /// the walker turns at its next waypoint, and the unit lies within 45° of the way, or 33.69°
    /// for one that stands, as Zero Hour's `blockedBy` takes it: the walker may step up to
    /// touching the unit, on by the step the unit is to take along the way, none for one that
    /// comes toward it, as `calculateMaxBlockedSpeed` brakes a mover to the speed of the one
    /// ahead; the touch is rounded toward the walker, so it ends touching or apart, but for the
    /// measures' own half a bit. The unit its walk goes into, one of another layer, and one it
    /// passes through block it not. Of two that block each other head on, both braked to a stop,
    /// the one of the lower weight, then of the higher stable id, yields, as
    /// `hasHigherPathPriority` ranks a vehicle over a soldier and then the lower id of two that
    /// face each other (entries-and-blocking.md, A head-on block steps the lower priority aside).
    pub(crate) fn brake(&self, id: StableId, way: &Going, step: Num) -> Brake {
        let mut brake = Brake {
            step: None,
            aside: None,
        };
        let Ok(at) = self.units.binary_search_by_key(&id, |unit| unit.id) else {
            return brake;
        };
        let walker = self.units[at];
        let bits = |value: Num| i128::from(value.to_bits());
        let full = bits(step);
        let Going { way, leg } = *way;
        let ahead_of_turn = leg.map_or(full, |leg| bits(leg).min(full));
        self.grid
            .visit_near(walker.position, step + walker.radius, |indexed| {
                let unit = self.units[indexed.id as usize];
                if unit.id == walker.id
                    || indexed.key != walker.layer
                    || walker.into == Some(unit.id)
                    || unit.into == Some(walker.id)
                    || Self::passes(&walker, &unit)
                {
                    return ControlFlow::Continue(());
                }
                let off = [
                    bits(unit.position.x - walker.position.x),
                    bits(unit.position.z - walker.position.z),
                ];
                let bearing = Bearing::of(&way, off);
                let held = match unit.way {
                    Some(_) => bearing.in_cone(),
                    None => bearing.in_narrow_cone(),
                };
                // A leg planned to pass the unit touching it measures a bit within its reach at
                // most, as each measure rounds within half a bit; it passes.
                let reach = bits(walker.radius + unit.radius);
                if !held || bearing.apart.abs() + 1 >= reach {
                    return ControlFlow::Continue(());
                }
                let across = (reach * reach - bearing.apart * bearing.apart).cast_unsigned();
                let root = i128::try_from(across.isqrt()).expect("a root of a reach");
                let touch = bearing.along - root - i128::from(root * root < across.cast_signed());
                if touch >= ahead_of_turn {
                    return ControlFlow::Continue(());
                }
                let ahead = unit.way.map_or(0, |unit_way| {
                    way.along(unit_way.scaled_away(bits(unit.pace))).max(0)
                });
                let allowed = (touch + ahead).clamp(0, full);
                let allowed = Num::from_bits(i64::try_from(allowed).expect("within a step"));
                brake.step = Some(brake.step.map_or(allowed, |step| step.min(allowed)));
                let yields = unit.weight > walker.weight
                    || (unit.weight == walker.weight && unit.id < walker.id);
                if allowed == Num::ZERO && yields && Self::head_on(&walker, &unit) {
                    let reach = walker.radius + unit.radius;
                    brake.aside = Some(brake.aside.map_or(reach, |aside| aside.max(reach)));
                }
                ControlFlow::Continue(())
            });
        brake
    }

    /// Whether `unit` walks at `walker`, which lies within the 45° cone of its way.
    fn head_on(walker: &Blocker, unit: &Blocker) -> bool {
        unit.way.is_some_and(|way| {
            let bits = |value: Num| i128::from(value.to_bits());
            let off = [
                bits(walker.position.x - unit.position.x),
                bits(walker.position.z - unit.position.z),
            ];
            Bearing::of(&way, off).in_cone()
        })
    }

    /// Whether `walker` passes through `unit`: one leaves its producer and the two are of a team,
    /// or both gather.
    fn passes(walker: &Blocker, unit: &Blocker) -> bool {
        let leaves =
            (walker.leaving || unit.leaving) && walker.team.is_some() && walker.team == unit.team;
        leaves || (walker.gathering && unit.gathering)
    }
}

#[cfg(test)]
mod tests;
