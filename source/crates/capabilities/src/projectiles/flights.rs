use campfire_math::{Num, Vec3};
use campfire_sim::{Position, StableId};

use crate::combat::damage::{Damage, DamageCause};
use crate::combat::pass_queue::PassQueue;
use crate::combat::targets::Targets;
use crate::deliveries::Deliveries;
use crate::deliveries::delivered::Delivered;
use crate::deliveries::hit::Hit;
use crate::projectiles::cast_hits::{CastHit, CastHits};
use crate::projectiles::projectile::{Flight, Payload, Projectile};
use crate::projectiles::projectile_spec::ProjectileSpec;
use crate::scripts::hook::Hook;
use crate::units::team::Team;

/// The flights of a tick: where their hits and ends go, the units each cast hit while its
/// projectiles fly, and a scratch list of the units a line's step meets, by share of the step.
#[derive(Debug)]
pub(crate) struct Flights<'a> {
    pub(crate) queue: &'a mut PassQueue,
    pub(crate) deliveries: &'a mut Deliveries,
    pub(crate) cast_hits: &'a mut CastHits,
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

impl Flights<'_> {
    /// Flies `aloft` a step; whether it ended. A homing one flies towards its target's position
    /// now, and hits it on arrival; one whose target is dead, gone or no target ends without a
    /// hit. One along a line hits each unit its type's `hits` selects whose body comes within
    /// half its width of this tick's path, in the order of the point of the path nearest each,
    /// then by stable id, each unit once, and once a cast for a type that says so; it ends at its
    /// first hit when its type stops on one, and at the end of its range.
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
                direction: Vec3::ZERO,
            };
            self.end(projectile, hit);
            return true;
        };
        let moved = from.get().step_toward(unit.pos.get(), spec.speed);
        let flown = flown + from.get().distance(moved);
        if moved != unit.pos.get() {
            *position =
                Position::new(moved).expect("a step ends between two points within the bound");
            projectile.fly_to(flown);
            return false;
        }
        let hit = Hit {
            delivery: Some(id),
            target: Some(target),
            pos: unit.pos,
            distance: flown,
            direction: from.get().direction_to(moved).unwrap_or(Vec3::ZERO),
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
            direction,
        };
        let offset = direction.checked_scale(step).expect("a step fits");
        let Some(to) = from.get().checked_add(offset).and_then(Position::new) else {
            self.end(projectile, line(from, flown));
            return true;
        };
        let group = match projectile.payload() {
            Payload::Action { group, .. } if spec.once_per_cast => Some(group),
            _ => None,
        };
        let metric = targets.metric();
        self.met.clear();
        for unit in targets.units() {
            let cast_hit = group.is_some_and(|group| {
                let hit = CastHit {
                    group,
                    unit: unit.id,
                };
                self.cast_hits.contains(hit)
            });
            let selects = spec
                .hits
                .selects(targets.attitude(team, unit.team), unit.tags);
            if projectile.struck(unit.id) || cast_hit || !selects {
                continue;
            }
            if let Some(share) = metric.meets(from, to, unit.pos, spec.width / 2 + unit.radius) {
                let along = (share.along << Num::FRAC_BITS)
                    .checked_div(share.length)
                    .unwrap_or(0);
                self.met.push((along, unit.id));
            }
        }
        self.met.sort_unstable();
        for at in 0..self.met.len() {
            let (along, unit) = self.met[at];
            let share = Num::from_bits(i64::try_from(along).expect("a share of one fits"));
            let travelled = step * share;
            let pos = from
                .get()
                .checked_add(direction.checked_scale(travelled).expect("within a step"))
                .and_then(Position::new)
                .expect("a point of a step within the bound");
            let hit = line(pos, flown + travelled);
            projectile.strike(unit);
            if let Some(group) = group {
                self.cast_hits.insert(CastHit { group, unit });
            }
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
            Payload::Attack { amount, kind, roll } => self.queue.push_damage(Damage {
                source: Some(projectile.source()),
                target,
                amount,
                kind,
                cause: DamageCause::Attack { roll },
                ability: None,
                depth: 0,
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
