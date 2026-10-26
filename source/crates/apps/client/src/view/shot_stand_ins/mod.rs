use bevy::camera::visibility::Visibility;
use bevy::ecs::entity::Entity;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::math::Vec3;
use bevy::transform::components::Transform;
use campfire_actions::{ActionBook, ActionSlots};
use campfire_combat::{Barrels, ShotSource, Turret, TurretAim};
use campfire_common::{Tick, Ticks};
use campfire_declarations::ById;
use campfire_host::ReceivedTick;
use campfire_projectiles::ProjectileSpec;
use campfire_sim::{Position, SimTick, StableId};
use campfire_units::{Facing, MapBook};
use campfire_values::UnitType;

use crate::view::drawing::Drawing;
use crate::view::effects::{FireSeen, ShotSeen};
use crate::view::float_num::FloatNum;
use crate::view::glide::{Glide, Spot};
use crate::view::map_metric::Lifts;
use crate::view::shot_dress::ShotDress;
use crate::view::{Heading, ground};

/// The ticks a stand-in's tick and the tick of the state its server's projectile arrives with may
/// lie apart: a frame may run several ticks, whose last the stand-in takes for its shot's, and
/// bring several states, whose newest the client takes for its projectile's, as Netcode for
/// Entities matches a predicted spawn within 5 ticks (02-engine-core.md, A predicted shot draws a
/// stand-in).
const SLACK: u64 = 5;

/// The stand-ins for the projectiles of the client's own units' shots, oldest first, each until
/// the server's projectile of its unit and type, of its tick within `SLACK`, takes its place, or
/// the server's state past that arrives with none (02-engine-core.md, A predicted shot draws a
/// stand-in).
#[derive(Resource, Debug, Default)]
pub(crate) struct ShotStandIns(Vec<StandIn>);

/// A stand-in: its drawing's root, its unit, its projectile's type, where it left its barrel and
/// its step a tick along the barrel's way, both drawn, and the tick it left at.
#[derive(Debug, Clone, Copy, PartialEq)]
struct StandIn {
    root: Entity,
    source: StableId,
    unit_type: UnitType,
    from: Vec3,
    step: Vec3,
    fired: Tick,
}

/// The books a stand-in's launch reads: the actions, and the barrels, turrets and projectiles of
/// the unit types, which a match of no combat or no projectiles lacks.
type LaunchBooks<'w> = (
    Res<'w, ActionBook>,
    Option<Res<'w, ById<UnitType, Barrels>>>,
    Option<Res<'w, ById<UnitType, Turret>>>,
    Option<Res<'w, ById<UnitType, ProjectileSpec>>>,
    Res<'w, MapBook>,
);

/// The units the client predicts, which alone fire shots ahead of the server's, with what their
/// shots' launches read.
type Shooters<'w, 's> = Query<
    'w,
    's,
    (
        &'static StableId,
        &'static UnitType,
        &'static ActionSlots,
        &'static Position,
        Option<&'static Facing>,
        Option<&'static TurretAim>,
        &'static Drawing,
    ),
>;

impl ShotStandIns {
    /// Draws a stand-in for the projectile of each shot of a unit the client predicts: at the
    /// launch the server gives the shot, from the barrel its slot launches from next, turned
    /// along the barrel's way, which it flies at its type's launch speed. A unit the client only
    /// receives fires no shot ahead of the server's, and a shot of no projectile or no barrel
    /// draws none.
    pub(crate) fn place(
        seen: On<'_, '_, ShotSeen>,
        (actions, barrels, turrets, specs, map): LaunchBooks<'_>,
        (tick, lifts): (Res<'_, SimTick>, Lifts<'_>),
        (shooters, mut fired): (Shooters<'_, '_>, Query<'_, '_, &mut FireSeen>),
        mut dress: ShotDress<'_>,
        mut stand_ins: ResMut<'_, Self>,
        mut commands: Commands<'_, '_>,
    ) {
        let ShotSeen { unit, slot, action } = *seen;
        let Ok((&source, &shooter, slots, position, facing, aim, drawing)) = shooters.get(unit)
        else {
            return;
        };
        let projectile = actions
            .get(action)
            .expect("a slot's action is in the book")
            .delivery
            .map(|delivery| delivery.unit_type);
        let speed = projectile.zip(specs).and_then(|(projectile, specs)| {
            let spec = specs.get(projectile)?;
            Some((projectile, spec.launch_speed()))
        });
        let barrels = barrels.as_ref().and_then(|barrels| barrels.get(shooter));
        let (Some((projectile, speed)), Some(barrels)) = (speed, barrels) else {
            return;
        };
        let Ok(mut turns) = fired.get_mut(drawing.root()) else {
            return;
        };
        let shot = ShotSource {
            turret: turrets.as_ref().and_then(|turrets| turrets.get(shooter)),
            slots,
            aim: aim.copied(),
            facing: facing.copied(),
            point: map.point(position.get()),
        };
        let Some(launch) = shot.launch(barrels, action, |count| turns.launches.turn(slot, count))
        else {
            return;
        };
        let from = ground(Position::from(map.position(launch.at)), lifts.get());
        let way = Vec3::new(
            launch.way.x.float(),
            launch.way.y.float(),
            launch.way.z.float(),
        );
        let rotation = Heading::along(way).unwrap_or_default();
        let mut root = commands.spawn((
            Transform::from_translation(from).with_rotation(rotation),
            Visibility::default(),
            Glide::ticked(Spot {
                point: from,
                yaw: 0.0,
            }),
        ));
        dress.dress(&mut root, projectile, None);
        stand_ins.0.push(StandIn {
            root: root.id(),
            source,
            unit_type: projectile,
            from,
            step: way * speed.float(),
            fired: tick.start(),
        });
    }

    /// Moves each stand-in to its place after the tick, along its way by its steps since it
    /// left, in every tick the client runs, as a predicted unit's drawing steps.
    pub(crate) fn step(
        stand_ins: Res<'_, Self>,
        tick: Res<'_, SimTick>,
        mut roots: Query<'_, '_, &mut Glide>,
    ) {
        let now = tick.start();
        for stand_in in &stand_ins.0 {
            let flown = now.since(stand_in.fired).map_or(0, Ticks::get);
            if let Ok(mut glide) = roots.get_mut(stand_in.root) {
                glide.tick(Spot {
                    point: stand_in.from + stand_in.step * flown as f32,
                    yaw: 0.0,
                });
            }
        }
    }

    /// Ends each stand-in whose shot the server did not fire: the state of a tick past its own
    /// by more than `SLACK` arrived, and no projectile took its place; and every one while the
    /// client holds no state, as it left its match or loads it again, whose server sends no
    /// projectile of a shot before.
    pub(crate) fn expire(
        mut stand_ins: ResMut<'_, Self>,
        received: Res<'_, ReceivedTick>,
        mut commands: Commands<'_, '_>,
    ) {
        let received = received.get();
        stand_ins.0.retain(|stand_in| {
            let unmatched = match received {
                Some(received) => received
                    .since(stand_in.fired)
                    .is_some_and(|late| late.get() > SLACK),
                None => true,
            };
            if unmatched {
                commands.entity(stand_in.root).despawn();
            }
            !unmatched
        });
    }

    /// The root of the stand-in that `source`'s projectile of `unit_type`, which arrived with the
    /// state of the tick `received`, takes the place of: of those of its unit and type within
    /// `SLACK` of that tick, the nearest, the oldest of two as near; it leaves the stand-ins. None
    /// when none stands for it.
    pub(crate) fn take(
        &mut self,
        source: StableId,
        unit_type: UnitType,
        received: Tick,
    ) -> Option<Entity> {
        let apart = |stand_in: &StandIn| received.get().abs_diff(stand_in.fired.get());
        let at = self
            .0
            .iter()
            .enumerate()
            .filter(|(_, stand_in)| stand_in.source == source && stand_in.unit_type == unit_type)
            .filter(|(_, stand_in)| apart(stand_in) <= SLACK)
            .min_by_key(|&(at, stand_in)| (apart(stand_in), at))
            .map(|(at, _)| at)?;
        Some(self.0.remove(at).root)
    }
}

#[cfg(test)]
mod tests;
