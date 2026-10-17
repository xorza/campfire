use std::f32::consts::FRAC_PI_2;

use bevy::app::{App, Plugin, Startup, Update};
use bevy::asset::{Assets, Handle};
use bevy::camera::Camera3d;
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::lifecycle::RemovedComponents;
use bevy::ecs::query::{Added, Allow, Changed, Has, Or, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut, Single};
use bevy::math::primitives::{Annulus, Plane3d};
use bevy::math::{Quat, Vec3};
use bevy::mesh::{Mesh, Mesh3d, Meshable};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::time::Time;
use bevy::transform::components::Transform;
use campfire_actions::{ActionProgress, ActionSlots, SlotCharges, SlotTimes};
use campfire_progression::{Learning, Points};
use campfire_sim::{EntityIndex, SimTick, Unpredicted};
use campfire_stats::{Level, Pools};
use campfire_units::{Dead, Life};
use campfire_values::{Rank, Team};

use crate::hud::gauge::{Cooling, Gauge, GaugeKind, SlotPlace};
use crate::hud::ring::Ring;
use crate::own_player::OwnPlayer;
use crate::view::ViewSystems;
use crate::view::drawing::Drawing;
use crate::view::look::Shape;

mod gauge;
mod ring;

/// Makes the match readable with plain shapes over the drawings: a bar of the life over every
/// unit with one; under the own avatar's, its other pools, a gold mark over each ability it may
/// learn a rank of, its cooldowns, a tick for each rank of an ability that ranks up, bright
/// once learned, and a tick for each charge of an ability with charges, bright while held and
/// filling while it comes back; a ring where each hit lands, and a ring under the unit the own avatar attacks.
/// It reads the sim's components and changes none.
#[derive(Debug)]
pub(crate) struct Hud;

/// Over a unit's drawing's top, the stand its gauges stand on, which turns to face the camera.
#[derive(Component, Debug)]
struct GaugeStand;

/// Whether a unit is of the client's team.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Allegiance {
    Own,
    Other,
}

/// The meshes and materials of the gauges and rings.
#[derive(Resource, Debug)]
struct HudPalette {
    quad: Handle<Mesh>,
    ring: Handle<Mesh>,
    back: Handle<StandardMaterial>,
    friend: Handle<StandardMaterial>,
    foe: Handle<StandardMaterial>,
    resource: Handle<StandardMaterial>,
    cooldown: Handle<StandardMaterial>,
    learnable: Handle<StandardMaterial>,
    hit: Handle<StandardMaterial>,
    target: Handle<StandardMaterial>,
}

/// The scratch `add_gauges` builds each unit's gauges in.
#[derive(Debug, Default)]
struct GaugeScratch {
    kinds: Vec<GaugeKind>,
    ranks: Vec<Ranked>,
    /// The own avatar's slots, as their rows show them.
    slots: Vec<SlotRows>,
}

impl GaugeScratch {
    /// Reads the rows of each of `slots`, in order, as `learning` gives them.
    fn read_slots(&mut self, slots: &ActionSlots, learning: Option<&Learning<'_>>) {
        self.slots.clear();
        self.slots.extend(slots.iter().map(|held| {
            SlotRows {
                ranks: learning.and_then(|learning| learning.ranks(held)),
                charges: learning
                    .and_then(|learning| learning.charges(held))
                    .map(|rule| rule.max.get()),
            }
        }));
    }

    /// The own avatar's ability rows from row `row`, for the slots `slots` holds in order: a learn
    /// mark over each ability that ranks up, when one does; a cooldown pip for each slot; the
    /// rank ticks of each ability that ranks up; and the charge ticks of each ability with
    /// charges.
    fn ability_rows(&mut self, mut row: u8) {
        let Self {
            kinds,
            ranks,
            slots,
        } = self;
        let of = u16::try_from(slots.len()).expect("a unit's slots fit u16");
        let at = |slot| SlotPlace { slot, of };
        let slots = slots.iter().copied();
        ranks.clear();
        ranks.extend((0..).zip(slots.clone()).filter_map(|(slot, held)| {
            Some(Ranked {
                slot,
                ranks: held.ranks?,
            })
        }));
        if !ranks.is_empty() {
            kinds.extend(ranks.iter().map(|ranked| GaugeKind::Learnable {
                at: at(ranked.slot),
                row,
            }));
            row += 1;
        }
        kinds.extend(
            (0..)
                .zip(slots.clone())
                .map(|(slot, _)| GaugeKind::Cooldown {
                    at: at(slot),
                    row,
                    cooling: None,
                }),
        );
        row += 1;
        for &Ranked { slot, ranks } in &*ranks {
            kinds.extend((1..=ranks).map(|rank| GaugeKind::Rank {
                at: at(slot),
                rank,
                ranks,
                row,
            }));
        }
        if !ranks.is_empty() {
            row += 1;
        }
        for (slot, held) in (0..).zip(slots) {
            let Some(max) = held.charges else {
                continue;
            };
            kinds.extend((1..=max).map(|charge| GaugeKind::Charge {
                at: at(slot),
                charge,
                max,
                row,
            }));
        }
    }
}

/// What an ability slot's rows show of it: the count of ranks of an ability that ranks up, and
/// the most charges of one with charges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SlotRows {
    ranks: Option<u8>,
    charges: Option<u8>,
}

/// An ability slot whose ability ranks up, and its count of ranks.
#[derive(Debug, Clone, Copy)]
struct Ranked {
    slot: u8,
    ranks: u8,
}

/// The drawn units with no gauges yet: each one's drawing, its team, its life, its other pools,
/// and its ability slots.
type Ungauged<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Drawing,
        &'static Team,
        Option<&'static Life>,
        Option<&'static Pools>,
        Option<&'static ActionSlots>,
    ),
    (Without<Gauged>, Allow<Unpredicted>),
>;

/// On a unit: its gauges are made, on their stand, when it has any; and on the own avatar, the
/// rows of the ability slots they were made for.
#[derive(Component, Debug, Default)]
struct Gauged {
    stand: Option<Entity>,
    slots: Option<Vec<SlotRows>>,
}

/// What the gauges show of each unit, and where they stand.
type Shown<'w, 's> = Query<
    'w,
    's,
    (
        &'static Gauged,
        Has<Dead>,
        (Option<&'static Life>, Option<&'static Pools>),
        (Option<&'static ActionSlots>, Option<&'static SlotTimes>),
        Option<&'static Points>,
        Option<&'static Level>,
    ),
    Allow<Unpredicted>,
>;

/// The gauged units whose gauges may show something else this frame: what they show changed, or
/// they are new.
type Restated<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        With<Gauged>,
        Or<(
            Changed<Life>,
            Changed<Pools>,
            Changed<ActionSlots>,
            Changed<SlotTimes>,
            Changed<Points>,
            Changed<Level>,
            Added<Dead>,
            Added<Gauged>,
        )>,
        Allow<Unpredicted>,
    ),
>;

/// The gauge stands, by the gauges they hold.
type Stands<'w, 's> = Query<'w, 's, &'static Children, With<GaugeStand>>;

/// The drawn units whose life changed.
type Hurt<'w, 's> = Query<
    'w,
    's,
    (&'static Life, &'static Drawing, &'static Gauged),
    (Changed<Life>, Allow<Unpredicted>),
>;

/// The fill of each gauge: a drawn quad that no gauge holds.
type Fills<'w, 's> = Query<'w, 's, &'static mut Transform, (With<Mesh3d>, Without<Gauge>)>;

/// The ring under the unit the own avatar attacks, a child of the root of that unit's drawing,
/// which the unit's despawn takes along.
#[derive(Debug, Clone, Copy)]
struct Marked {
    root: Entity,
    ring: Entity,
}

/// How far over a drawing's top its gauges start.
const ABOVE: f32 = 0.35;

impl Plugin for Hud {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, Self::set_palette);
        app.add_systems(
            Update,
            (
                Self::regauge,
                Self::add_gauges,
                Self::lift_stands,
                Self::face_camera,
                Self::mark_hits,
                Self::fill_gauges,
                Self::widen_rings,
                Self::mark_target,
            )
                .chain()
                .after(ViewSystems),
        );
    }
}

impl Hud {
    fn set_palette(
        mut commands: Commands<'_, '_>,
        mut meshes: ResMut<'_, Assets<Mesh>>,
        mut materials: ResMut<'_, Assets<StandardMaterial>>,
    ) {
        let mut flat = |color: Color| {
            materials.add(StandardMaterial {
                base_color: color,
                unlit: true,
                ..StandardMaterial::default()
            })
        };
        commands.insert_resource(HudPalette {
            quad: meshes.add(Plane3d::default().mesh().size(1.0, 1.0)),
            ring: meshes.add(Annulus::new(0.55, 0.7).mesh()),
            back: flat(Color::srgb(0.1, 0.1, 0.12)),
            friend: flat(Color::srgb(0.3, 0.85, 0.35)),
            foe: flat(Color::srgb(0.9, 0.3, 0.25)),
            resource: flat(Color::srgb(0.3, 0.55, 1.0)),
            cooldown: flat(Color::srgb(0.9, 0.9, 0.9)),
            learnable: flat(Color::srgb(1.0, 0.72, 0.1)),
            hit: flat(Color::srgb(1.0, 0.8, 0.3)),
            target: flat(Color::srgb(1.0, 0.35, 0.2)),
        });
    }

    /// Gives each drawn unit its gauges, once the client plays or watches, its side telling
    /// friend from foe: life for every unit with a life; for the own avatar, each other pool,
    /// then, when an ability ranks up, a row of learn marks, then a cooldown per ability slot,
    /// then the rank ticks of each ability that ranks up, then the charge ticks of each ability
    /// with charges. The gauges stand over the drawing's top, facing the camera, as children of
    /// its root.
    fn add_gauges(
        palette: Res<'_, HudPalette>,
        learning: Option<Learning<'_>>,
        own: OwnPlayer<'_, '_>,
        drawn: Ungauged<'_, '_>,
        shapes: Query<'_, '_, &Shape>,
        mut scratch: Local<'_, GaugeScratch>,
        mut commands: Commands<'_, '_>,
    ) {
        let Some(own_team) = own.side() else {
            return;
        };
        let avatar = own.avatar().map(|avatar| avatar.entity);
        for (unit, drawing, team, life, pools, slots) in &drawn {
            // A projectile's drawing has no look, and no gauges.
            let Ok(shape) = shapes.get(drawing.root()) else {
                commands.entity(unit).insert(Gauged::default());
                continue;
            };
            let mine = Some(unit) == avatar;
            let allegiance = if *team == own_team {
                Allegiance::Own
            } else {
                Allegiance::Other
            };
            scratch.kinds.clear();
            if let Some(life) = life {
                scratch.kinds.push(GaugeKind::Life {
                    shown: Some(life.current()),
                });
            }
            let mut row = 1;
            if mine && let Some(pools) = pools {
                for pool in pools.ids() {
                    scratch.kinds.push(GaugeKind::Pool { pool, row });
                    row += 1;
                }
            }
            let rows = if mine && let Some(slots) = slots {
                scratch.read_slots(slots, learning.as_ref());
                scratch.ability_rows(row);
                Some(scratch.slots.clone())
            } else {
                None
            };
            if scratch.kinds.is_empty() {
                commands.entity(unit).insert(Gauged {
                    stand: None,
                    slots: rows,
                });
                continue;
            }
            let top = Vec3::Y * (shape.height + ABOVE);
            let stand = commands
                .spawn((
                    GaugeStand,
                    Transform::from_translation(top),
                    Visibility::Inherited,
                ))
                .id();
            commands.entity(drawing.root()).add_child(stand);
            commands.entity(unit).insert(Gauged {
                stand: Some(stand),
                slots: rows,
            });
            for &kind in &scratch.kinds {
                let layout = kind.layout();
                let back = commands
                    .spawn((
                        Mesh3d(palette.quad.clone()),
                        MeshMaterial3d(palette.back.clone()),
                        layout.back(),
                    ))
                    .id();
                let fill = commands
                    .spawn((
                        Mesh3d(palette.quad.clone()),
                        MeshMaterial3d(palette.fill(kind, allegiance).clone()),
                        layout.fill(0.0),
                    ))
                    .id();
                let gauge = commands
                    .spawn((Gauge { kind, fill }, layout.place(), Visibility::Hidden))
                    .add_children(&[back, fill])
                    .id();
                commands.entity(stand).add_child(gauge);
            }
        }
    }

    /// Takes the own avatar's gauges down when its ability slots change their rows, as an
    /// ability or an item joins or leaves them, or a rank gives charges, so `add_gauges` makes
    /// them again.
    fn regauge(
        learning: Option<Learning<'_>>,
        avatars: Query<'_, '_, (Entity, &Gauged, &ActionSlots), Changed<ActionSlots>>,
        mut scratch: Local<'_, GaugeScratch>,
        mut commands: Commands<'_, '_>,
    ) {
        for (unit, gauged, slots) in &avatars {
            let Some(rows) = &gauged.slots else {
                continue;
            };
            scratch.read_slots(slots, learning.as_ref());
            if scratch.slots == *rows {
                continue;
            }
            if let Some(stand) = gauged.stand {
                commands.entity(stand).despawn();
            }
            commands.entity(unit).remove::<Gauged>();
        }
    }

    /// Stands each unit's gauges over its drawing's top as its shape's height changes, as a
    /// model drawing's does once its meshes have their bounds.
    fn lift_stands(
        roots: Query<'_, '_, (&Shape, &Children), Changed<Shape>>,
        mut stands: Query<'_, '_, &mut Transform, With<GaugeStand>>,
    ) {
        for (shape, children) in &roots {
            let mut lifted = stands.iter_many_mut(children);
            while let Some(mut place) = lifted.fetch_next() {
                place.translation = Vec3::Y * (shape.height + ABOVE);
            }
        }
    }

    /// Turns every gauge stand to face the camera: its up, the gauges' face, toward the camera's
    /// back, its right along the camera's. A drawing's root never turns, so a stand's own turn is
    /// its turn in the world.
    fn face_camera(
        camera: Single<'_, '_, &Transform, (With<Camera3d>, Without<GaugeStand>)>,
        mut stands: Query<'_, '_, &mut Transform, With<GaugeStand>>,
    ) {
        let facing = camera.rotation * Quat::from_rotation_x(FRAC_PI_2);
        for mut stand in &mut stands {
            if stand.rotation != facing {
                stand.rotation = facing;
            }
        }
    }

    /// Puts a ring on the ground where a unit's life dropped since its gauge last showed it.
    fn mark_hits(
        time: Res<'_, Time>,
        palette: Res<'_, HudPalette>,
        units: Hurt<'_, '_>,
        roots: Query<'_, '_, &Transform, Without<GaugeStand>>,
        stands: Stands<'_, '_>,
        mut gauges: Query<'_, '_, &mut Gauge>,
        mut commands: Commands<'_, '_>,
    ) {
        for (life, drawing, Gauged { stand, .. }) in &units {
            let (Some(stand), Ok(root)) = (*stand, roots.get(drawing.root())) else {
                continue;
            };
            let Ok(children) = stands.get(stand) else {
                continue;
            };
            let mut gauges = gauges.iter_many_mut(children);
            while let Some(mut gauge) = gauges.fetch_next() {
                let GaugeKind::Life { shown } = &mut gauge.kind else {
                    continue;
                };
                let current = life.current();
                if shown.is_some_and(|shown| current < shown) {
                    commands.spawn((
                        Mesh3d(palette.ring.clone()),
                        MeshMaterial3d(palette.hit.clone()),
                        Transform::from_translation(root.translation + Vec3::Y * 0.05)
                            .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                        Ring {
                            since: time.elapsed(),
                        },
                    ));
                }
                *shown = Some(current);
            }
        }
    }

    /// Sets the gauges' fills of each unit whose gauges may show something else, and of the own
    /// avatar, whose cooldowns fill with the ticks; hides the gauges of the dead, of unlearned
    /// abilities' cooldowns, and the learn marks of the abilities the unit may not learn now. A
    /// dead unit may learn, so its learn marks show.
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    fn fill_gauges(
        tick: Option<Res<'_, SimTick>>,
        own: OwnPlayer<'_, '_>,
        learning: Option<Learning<'_>>,
        restated: Restated<'_, '_>,
        mut revived: RemovedComponents<'_, '_, Dead>,
        units: Shown<'_, '_>,
        stands: Stands<'_, '_>,
        (mut gauges, mut fills): (Query<'_, '_, (&mut Gauge, &mut Visibility)>, Fills<'_, '_>),
        mut touched: Local<'_, Vec<Entity>>,
    ) {
        let avatar = own.avatar().map(|avatar| avatar.entity);
        touched.clear();
        touched.extend(restated.iter().chain(revived.read()).chain(avatar));
        touched.sort_unstable();
        touched.dedup();
        let now = tick.map(|tick| tick.start());
        for &unit in &*touched {
            let Ok((Gauged { stand, .. }, dead, (life, pools), (slots, times), points, level)) =
                units.get(unit)
            else {
                continue;
            };
            let Some(Ok(children)) = stand.map(|stand| stands.get(stand)) else {
                continue;
            };
            let mut gauges = gauges.iter_many_mut(children);
            while let Some((mut gauge, mut visibility)) = gauges.fetch_next() {
                let fill = |pool| {
                    let pools = pools?;
                    Some(Gauge::share(pools.current(pool)?, pools.max(pool)?))
                };
                let fraction = match &mut gauge.kind {
                    GaugeKind::Life { .. } => {
                        life.map(|life| Gauge::share(life.current(), life.max()))
                    }
                    GaugeKind::Pool { pool, .. } => fill(*pool),
                    GaugeKind::Cooldown {
                        at: SlotPlace { slot, .. },
                        cooling,
                        ..
                    } => {
                        let learned = slots
                            .and_then(|slots| slots.slot(*slot))
                            .filter(|held| held.rank.is_some());
                        let time = learned.and(times).and_then(|times| times.slot(*slot));
                        time.zip(now).map(|(time, now)| {
                            if cooling.is_none_or(|cooling| cooling.ready_at != time.ready_at) {
                                *cooling = Some(Cooling {
                                    ready_at: time.ready_at,
                                    seen_at: now,
                                });
                            }
                            cooling.map_or(1.0, |cooling| cooling.filled(now))
                        })
                    }
                    GaugeKind::Rank {
                        at: SlotPlace { slot, .. },
                        rank,
                        ..
                    } => slots.and_then(|slots| slots.slot(*slot)).map(|held| {
                        if Rank::count(held.rank) >= *rank {
                            1.0
                        } else {
                            0.0
                        }
                    }),
                    GaugeKind::Charge {
                        at: SlotPlace { slot, .. },
                        charge,
                        ..
                    } => {
                        let held = slots.and_then(|slots| slots.slot(*slot));
                        let rule = learning
                            .as_ref()
                            .zip(held)
                            .and_then(|(learning, held)| learning.charges(held));
                        let time = times.and_then(|times| times.slot(*slot));
                        time.zip(rule).zip(now).map(|((time, rule), now)| {
                            // A slot that spent none yet holds every charge.
                            let charges = time.charges.unwrap_or_else(|| SlotCharges {
                                count: rule.max.get(),
                                next: now,
                            });
                            GaugeKind::charge_fill(*charge, charges, rule, now)
                        })
                    }
                    GaugeKind::Learnable {
                        at: SlotPlace { slot, .. },
                        ..
                    } => {
                        let held = slots.and_then(|slots| slots.slot(*slot));
                        let learnable = learning
                            .as_ref()
                            .zip(held)
                            .zip(points.zip(level))
                            .is_some_and(|((learning, held), (&points, &level))| {
                                learning.learnable(held, points, level)
                            });
                        learnable.then_some(1.0)
                    }
                };
                let learns = matches!(gauge.kind, GaugeKind::Learnable { .. });
                let shown = fraction.filter(|_| !dead || learns);
                visibility.set_if_neq(if shown.is_some() {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
                if let (Some(fraction), Ok(mut fill)) = (shown, fills.get_mut(gauge.fill)) {
                    fill.set_if_neq(gauge.kind.layout().fill(fraction));
                }
            }
        }
    }

    /// Puts the target ring on the ground under the unit the own avatar attacks, as wide as the
    /// unit's drawing, when the target changes: a child of the target's drawing, which carries it
    /// along. A ring whose unit is gone went with the unit's drawing.
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    fn mark_target(
        index: Res<'_, EntityIndex>,
        palette: Res<'_, HudPalette>,
        own: OwnPlayer<'_, '_>,
        slots: Query<'_, '_, &ActionProgress>,
        units: Query<'_, '_, &Drawing, Allow<Unpredicted>>,
        shapes: Query<'_, '_, &Shape>,
        mut marked: Local<'_, Option<Marked>>,
        mut commands: Commands<'_, '_>,
    ) {
        let target = own
            .avatar()
            .and_then(|avatar| slots.get(avatar.entity).ok())
            .and_then(ActionProgress::attack_target)
            .and_then(|target| index.get(target))
            .and_then(|target| units.get(target).ok())
            .and_then(|drawing| Some((drawing.root(), shapes.get(drawing.root()).ok()?)));
        if marked.map(|marked| marked.root) == target.map(|(root, _)| root) {
            return;
        }
        if let Some(old) = marked.take() {
            commands.entity(old.ring).try_despawn();
        }
        if let Some((root, shape)) = target {
            let ring = commands
                .spawn((
                    Mesh3d(palette.ring.clone()),
                    MeshMaterial3d(palette.target.clone()),
                    Transform::from_translation(Vec3::Y * 0.04)
                        .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
                        .with_scale(Vec3::splat(2.0 * shape.reach())),
                    ChildOf(root),
                ))
                .id();
            *marked = Some(Marked { root, ring });
        }
    }

    fn widen_rings(
        time: Res<'_, Time>,
        mut rings: Query<'_, '_, (Entity, &Ring, &mut Transform)>,
        mut commands: Commands<'_, '_>,
    ) {
        for (entity, ring, mut transform) in &mut rings {
            match ring.scale(time.elapsed()) {
                Some(scale) => transform.scale = Vec3::splat(scale),
                None => commands.entity(entity).despawn(),
            }
        }
    }
}

impl HudPalette {
    /// The material a gauge of `kind` fills with, for a unit of `allegiance`.
    const fn fill(&self, kind: GaugeKind, allegiance: Allegiance) -> &Handle<StandardMaterial> {
        match (kind, allegiance) {
            (GaugeKind::Life { .. }, Allegiance::Own) => &self.friend,
            (GaugeKind::Life { .. }, Allegiance::Other) => &self.foe,
            (GaugeKind::Pool { .. }, _) => &self.resource,
            (GaugeKind::Learnable { .. }, _) => &self.learnable,
            (GaugeKind::Cooldown { .. } | GaugeKind::Rank { .. } | GaugeKind::Charge { .. }, _) => {
                &self.cooldown
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::asset::{AssetApp, AssetPlugin};
    use bevy::time::{TimePlugin, TimeUpdateStrategy};
    use campfire_actions::SlotKind;
    use campfire_math::Num;
    use campfire_sim::{IdAllocator, Position};
    use campfire_units::BodyUnit;
    use campfire_values::{ActionId, UnitType};

    use super::*;
    use crate::selection::Selection;
    use crate::view::View;
    use crate::view::client_data::ClientData;

    #[test]
    fn a_ring_marks_each_drop_of_life_where_the_unit_is_drawn() {
        // Bevy checks that a system's queries do not conflict when it first runs.
        let mut app = App::new();
        app.add_plugins((TimePlugin, AssetPlugin::default()));
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.init_resource::<EntityIndex>();
        app.init_resource::<Selection>();
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        Unpredicted::register(app.world_mut());
        app.init_resource::<ClientData>();
        app.add_plugins((View, Hud));
        app.update();
        assert!(app.world().contains_resource::<HudPalette>());
        // With no own avatar there is no target to mark: nothing wears the target's material.
        let target = app.world().resource::<HudPalette>().target.clone();
        let mut worn = app.world_mut().query::<&MeshMaterial3d<StandardMaterial>>();
        assert!(worn.iter(app.world()).all(|material| material.0 != target));

        // A unit of 600 life at (3, 0, −2), whose life gauge showed it full.
        let life = |max| Life::full(Num::int(max));
        let pos = Position::from((3, 0, -2));
        let unit = app
            .world_mut()
            .spawn((
                IdAllocator::default().allocate(),
                BodyUnit,
                pos,
                Team::new(0),
                // A type the client data draws with no model, as every type of a view with no
                // looks.
                UnitType::new(0),
                life(600),
            ))
            .id();
        app.update();
        let root = app.world().get::<Drawing>(unit).unwrap().root();
        let shown = |shown| GaugeKind::Life {
            shown: Some(Num::int(shown)),
        };
        let stand = app
            .world_mut()
            .spawn((GaugeStand, Transform::default(), ChildOf(root)))
            .id();
        let gauge = app
            .world_mut()
            .spawn((
                Gauge {
                    kind: shown(600),
                    fill: Entity::PLACEHOLDER,
                },
                ChildOf(stand),
            ))
            .id();
        app.world_mut().entity_mut(unit).insert(Gauged {
            stand: Some(stand),
            slots: None,
        });
        let mut rings = app.world_mut().query::<(&Ring, &Transform)>();
        // Unchanged, its life marks nothing; a drop to 450 marks one ring on the ground under it,
        // a rise to 500 none, and the gauge keeps what it showed last.
        app.update();
        assert_eq!(rings.iter(app.world()).count(), 0);
        app.world_mut().entity_mut(unit).insert(life(450));
        app.update();
        let (_, ring) = rings.single(app.world()).unwrap();
        assert_eq!(ring.translation, Vec3::new(3.0, 0.05, -2.0));
        assert_eq!(app.world().get::<Gauge>(gauge).unwrap().kind, shown(450));
        app.world_mut().entity_mut(unit).insert(life(500));
        app.update();
        assert_eq!(app.world().get::<Gauge>(gauge).unwrap().kind, shown(500));
        assert_eq!(rings.iter(app.world()).count(), 1);

        // Its drawing grows to 5 m, as a model's bounds come: the stand stands 0.35 m over it.
        app.world_mut().get_mut::<Shape>(root).unwrap().height = 5.0;
        app.update();
        let place = app.world().get::<Transform>(stand).unwrap().translation;
        assert_eq!(place, Vec3::Y * 5.35);

        // As the own avatar, its gauges made for one slot of no ranks and no charges: a change of
        // its slots that keeps those rows keeps them, and a second slot takes them down for
        // `add_gauges`, stand and all.
        let slot = |at| (ActionId::new(at), SlotKind::new(0), Some(Rank::FIRST));
        let none = SlotRows {
            ranks: None,
            charges: None,
        };
        app.world_mut().entity_mut(unit).insert((
            Gauged {
                stand: Some(stand),
                slots: Some(vec![none]),
            },
            ActionSlots::new([slot(0)]),
        ));
        app.update();
        app.world_mut()
            .entity_mut(unit)
            .insert(ActionSlots::new([slot(1)]));
        app.update();
        assert!(app.world().get::<Gauged>(unit).is_some());
        app.world_mut()
            .entity_mut(unit)
            .insert(ActionSlots::new([slot(0), slot(1)]));
        app.update();
        assert!(app.world().get::<Gauged>(unit).is_none());
        assert!(app.world().get_entity(stand).is_err());
    }

    #[test]
    fn an_avatars_ability_rows_stack_its_marks_cooldowns_ranks_and_charges() {
        // Three slots: one of 3 ranks; one of 2 charges; one of neither. Row 1 holds the learn
        // mark over the ranked slot, row 2 a cooldown pip for each slot, row 3 the ranked slot's
        // 3 ticks, row 4 the charged slot's 2.
        let slots = [
            SlotRows {
                ranks: Some(3),
                charges: None,
            },
            SlotRows {
                ranks: None,
                charges: Some(2),
            },
            SlotRows {
                ranks: None,
                charges: None,
            },
        ];
        let mut scratch = GaugeScratch {
            slots: slots.to_vec(),
            ..GaugeScratch::default()
        };
        scratch.ability_rows(1);
        // Each of the three slots' gauges knows the row of three it centers on.
        let at = |slot| SlotPlace { slot, of: 3 };
        let cooldown = |slot| GaugeKind::Cooldown {
            at: at(slot),
            row: 2,
            cooling: None,
        };
        let rank = |rank| GaugeKind::Rank {
            at: at(0),
            rank,
            ranks: 3,
            row: 3,
        };
        let charge = |charge| GaugeKind::Charge {
            at: at(1),
            charge,
            max: 2,
            row: 4,
        };
        assert_eq!(
            scratch.kinds,
            [
                GaugeKind::Learnable { at: at(0), row: 1 },
                cooldown(0),
                cooldown(1),
                cooldown(2),
                rank(1),
                rank(2),
                rank(3),
                charge(1),
                charge(2),
            ]
        );
        // With no ability that ranks up, the cooldowns take the first row and the charges the
        // next; the two slots center on a row of two.
        let mut scratch = GaugeScratch {
            slots: slots[1..].to_vec(),
            ..GaugeScratch::default()
        };
        scratch.ability_rows(1);
        let at = |slot| SlotPlace { slot, of: 2 };
        let cooldown = |slot| GaugeKind::Cooldown {
            at: at(slot),
            row: 1,
            cooling: None,
        };
        let charge = |charge| GaugeKind::Charge {
            at: at(0),
            charge,
            max: 2,
            row: 2,
        };
        assert_eq!(
            scratch.kinds,
            [cooldown(0), cooldown(1), charge(1), charge(2)]
        );
    }
}
