use campfire_math::{Num, U256, Vec3};
use campfire_sim::{Position, StableId};

use crate::actions::targets::Targets;
use crate::combat::damage::{Damage, DamageCause};
use crate::combat::pass_queue::PassQueue;
use crate::deliveries::Deliveries;
use crate::deliveries::delivered::Delivered;
use crate::projectiles::projectile::{Flight, Payload, Projectile};
use crate::projectiles::projectile_spec::ProjectileSpec;
use crate::projectiles::struck_units::{Struck, StruckUnits};
use crate::scripts::hook::Hook;
use crate::units::body_grid::BodyGrid;
use crate::units::team::Team;
use crate::values::hit::Hit;

/// The flights of a tick: where their hits and ends go, the units each line struck while it
/// flies, the bodies a line may meet, indexed as the stage began, and a scratch list
/// of the units a line's step meets, by share of the step.
#[derive(Debug)]
pub(crate) struct Flights<'a> {
    pub(crate) queue: &'a mut PassQueue,
    pub(crate) deliveries: &'a mut Deliveries,
    pub(crate) struck: &'a mut StruckUnits,
    pub(crate) grid: &'a BodyGrid<()>,
    pub(crate) met: &'a mut Vec<(u128, StableId)>,
}

/// A projectile in flight this tick: its stable id, its team, its type's spec, where it is, and
/// itself.
#[derive(Debug)]
pub(crate) struct Aloft<'a> {
    pub(crate) id: StableId,
    pub(crate) team: Team,
    pub(crate) spec: ProjectileSpec,
    pub(crate) position: &'a mut Position,
    pub(crate) projectile: &'a mut Projectile,
}

impl Aloft<'_> {
    /// What its hits are kept by: its cast for a type that strikes a unit once a cast, or
    /// itself.
    pub(crate) const fn strikes_by(&self) -> StableId {
        match self.projectile.payload() {
            Payload::Action { group, .. } if self.spec.once_per_cast => group,
            _ => self.id,
        }
    }
}

impl Flights<'_> {
    /// Flies `aloft` a step; whether it ended. A homing one flies towards its target's position
    /// now, and hits it where its step ends when its body, of half its width, then reaches the
    /// target's; one whose target is dead, gone or no target ends without a hit. One along a line
    /// hits each unit its type's `hits` selects whose body comes within half its width of this
    /// tick's path, in the order of the point of the path nearest each, then by stable id, each
    /// unit once, and once a cast for a type that says so; it ends at its first hit when its type
    /// stops on one, and at the end of its range.
    pub(crate) fn fly(&mut self, targets: &Targets<'_, '_>, aloft: Aloft<'_>) -> bool {
        match aloft.projectile.flight() {
            Flight::Homing { target, flown } => self.homing(targets, aloft, target, flown),
            Flight::Line {
                direction,
                flown,
                range,
                aimed,
            } => self.line(targets, aloft, direction, flown, range, aimed),
        }
    }

    fn homing(
        &mut self,
        targets: &Targets<'_, '_>,
        aloft: Aloft<'_>,
        target: StableId,
        flown: Num,
    ) -> bool {
        let Aloft {
            id,
            spec,
            position,
            projectile,
            ..
        } = aloft;
        let from = *position;
        let Some(unit) = targets.living(target) else {
            let hit = Hit {
                delivery: Some(id),
                target: Some(target),
                pos: from,
                distance: flown,
                direction: None,
            };
            self.end(projectile, hit);
            return true;
        };
        let metric = targets.metric();
        let moved = metric.step_toward(from, unit.pos, spec.speed);
        let offset = metric.offset(from, moved);
        let flown = flown + offset.length();
        if !metric.reaches(moved, spec.width / 2, Num::ZERO, unit.pos, unit.radius) {
            *position = moved;
            projectile.fly_to(flown);
            return false;
        }
        let hit = Hit {
            delivery: Some(id),
            target: Some(target),
            pos: moved,
            distance: flown,
            direction: offset.normalized(),
        };
        self.strike(projectile, target, hit);
        self.end(projectile, hit);
        true
    }

    /// Flies `aloft` a step along `direction`, `flown` meters of its `range`; its hits and its end
    /// name `aimed` as their action's target.
    fn line(
        &mut self,
        targets: &Targets<'_, '_>,
        aloft: Aloft<'_>,
        direction: Vec3,
        flown: Num,
        range: Num,
        aimed: Option<StableId>,
    ) -> bool {
        let by = aloft.strikes_by();
        let Aloft {
            id,
            team,
            spec,
            position,
            projectile,
        } = aloft;
        let from = *position;
        let step = spec.speed.min(range - flown);
        let line = |pos, distance| Hit {
            delivery: Some(id),
            target: aimed,
            pos,
            distance,
            direction: Some(direction),
        };
        let offset = direction.checked_scale(step).expect("a step fits");
        let Some(to) = from.get().checked_add(offset).and_then(Position::new) else {
            self.end(projectile, line(from, flown));
            return true;
        };
        let metric = targets.metric();
        self.met.clear();
        let (start, end) = (from.get(), to.get());
        let half = spec.width / 2;
        let low = [start.x.min(end.x), start.z.min(end.z)]
            .map(|axis| axis.checked_sub(half).unwrap_or(Num::MIN));
        let high = [start.x.max(end.x), start.z.max(end.z)]
            .map(|axis| axis.checked_add(half).unwrap_or(Num::MAX));
        let (met, struck) = (&mut *self.met, &*self.struck);
        self.grid.visit(low, high, |body| {
            let Some(unit) = targets.living(body.id) else {
                return;
            };
            let selects = spec
                .hits
                .selects(targets.attitude(team, unit.team), unit.tags);
            if !selects || struck.contains(Struck { by, unit: unit.id }) {
                return;
            }
            if let Some(share) = metric.meets(from, to, unit.pos, half + unit.radius) {
                met.push((share.along, unit.id));
            }
        });
        // Every share of a step has the step's squared length below it, so the raw `along`
        // orders the nearest points exactly.
        self.met.sort_unstable();
        let length = metric.offset(from, to).length_squared_bits();
        for at in 0..self.met.len() {
            let (along, unit) = self.met[at];
            let travelled = if length == 0 {
                Num::ZERO
            } else {
                let step_bits = u128::try_from(step.to_bits()).expect("a step is never negative");
                let bits = U256::product(step_bits, along)
                    .round_div(length)
                    .expect("a share of a step fits");
                Num::from_bits(i64::try_from(bits).expect("a share of a step fits"))
            };
            let pos = from
                .get()
                .checked_add(direction.checked_scale(travelled).expect("within a step"))
                .and_then(Position::new)
                .expect("a point of a step within the bound");
            let hit = line(pos, flown + travelled);
            self.struck.insert(Struck { by, unit });
            self.strike(projectile, unit, hit);
            if spec.stop_on_hit {
                self.end(projectile, hit);
                return true;
            }
        }
        *position = to;
        projectile.fly_to(flown + step);
        if flown + step < range {
            return false;
        }
        self.end(projectile, line(to, range));
        true
    }

    /// What a hit of `target` delivers: an attack's damage, its roll with it, or its action's
    /// `on_hit`.
    fn strike(&mut self, projectile: &Projectile, target: StableId, hit: Hit) {
        match projectile.payload() {
            Payload::Attack {
                action,
                amount,
                kind,
                roll,
            } => self.queue.push_damage(Damage {
                source: Some(projectile.source()),
                target,
                amount,
                kind,
                cause: DamageCause::Attack { roll },
                ability: Some(action),
                depth: 0,
                hit: Some(hit),
            }),
            Payload::Action { action, rank, .. } => self.deliveries.delivered.push(Delivered {
                source: projectile.source(),
                action,
                rank,
                hook: Hook::OnHit,
                reached: Some(target),
                hit,
            }),
        }
    }

    /// An action's projectile ends as `hit` says: its `on_end` runs. An attack's ends with
    /// nothing more.
    fn end(&mut self, projectile: &Projectile, hit: Hit) {
        if let Payload::Action { action, rank, .. } = projectile.payload() {
            self.deliveries.delivered.push(Delivered {
                source: projectile.source(),
                action,
                rank,
                hook: Hook::OnEnd,
                reached: None,
                hit,
            });
        }
    }
}
