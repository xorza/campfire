use std::f32::consts::FRAC_PI_2;

use bevy::app::{App, Plugin, Startup, Update};
use bevy::asset::{AssetPath, AssetServer, Assets, Handle};
use bevy::camera::visibility::Visibility;
use bevy::camera::{Camera3d, Projection};
use bevy::color::Color;
use bevy::ecs::change_detection::{DetectChanges, DetectChangesMut};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::lifecycle::RemovedComponents;
use bevy::ecs::query::{Added, Allow, Changed, Has, Or, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::common_conditions::{not, resource_exists};
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut, Single};
use bevy::ecs::world::Ref;
use bevy::image::Image;
use bevy::material::AlphaMode;
use bevy::math::{Affine2, Quat, Rect, Vec2, Vec3};
use bevy::mesh::{Mesh, Mesh3d, Meshable};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::shape::{Annulus, Plane3d};
use bevy::time::Time;
use bevy::transform::components::Transform;
use bevy::window::{PrimaryWindow, Window};
use campfire_actions::{ActionBook, ActionProgress, ActionSlots, SlotCharges, SlotTimes};
use campfire_client_files::{HitMark, ShownBars};
use campfire_progression::{Learning, Points};
use campfire_sim::{EntityIndex, SimTick, StableId, Unpredicted};
use campfire_stats::{Level, Pools};
use campfire_units::{Dead, Leader, Life, LivingSpawns};
use campfire_values::{PackageIndex, Rank, Relation, Relations, Team};

use crate::hud::gauge::{Cooling, Gauge, GaugeKind, Layout, SlotPlace};
use crate::hud::ring::Ring;
use crate::own_player::OwnPlayer;
use crate::pointer::Pointer;
use crate::selection::{Selection, SelectionSystems};
use crate::view::ViewSystems;
use crate::view::drawing::{Drawing, DrawingOf};
use crate::view::look::UnitShape;
use crate::view::match_hud::MatchHud;
use crate::view::package_source::PackageAssets;
use crate::view::pixel_scale::PixelScale;

mod gauge;
mod ring;

/// Makes the match readable with plain shapes over the drawings: a bar of the life over every unit
/// with one, or, as the mode's HUD file says, over the selected and the pointed alone, beside it the
/// icon of the unit's level the HUD file gives (generals.md, A level shows its icon); under the
/// own avatar's, its other pools, a gold mark over each ability it may learn a rank of, its
/// cooldowns, a tick for each rank of an ability that ranks up, bright once learned, and a tick for
/// each charge of an ability with charges, bright while held and filling while it comes back; a
/// ring where each hit lands; and one overlay of rings under units, one under each unit the player
/// selected and a wider one under the unit the own avatar attacks. It reads the sim's components
/// and changes none, and keeps its own on its drawings.
#[derive(Debug)]
pub(crate) struct HudPlugin;

/// Over a unit's drawing's top, the stand its gauges stand on, which turns to face the camera.
#[derive(Component, Debug)]
struct GaugeStand;

/// The quad of a gauge that fills as its share grows, over the gauge's back.
#[derive(Component, Debug)]
struct GaugeFill;

/// Whether the client's side regards a unit's team as friendly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Allegiance {
    Own,
    Other,
}

/// The meshes and materials of the gauges and rings.
#[derive(Resource, Debug)]
struct HudPalette {
    quad: Handle<Mesh>,
    /// A flat ring whose outer radius is 1, which a hit's ring scales to its unit's reach.
    ring: Handle<Mesh>,
    /// A flat ring whose inner radius is 1, which a unit's ring scales to the unit's reach.
    unit_ring: Handle<Mesh>,
    back: Handle<StandardMaterial>,
    friend: Handle<StandardMaterial>,
    foe: Handle<StandardMaterial>,
    resource: Handle<StandardMaterial>,
    cooldown: Handle<StandardMaterial>,
    learnable: Handle<StandardMaterial>,
    hit: Handle<StandardMaterial>,
    target: Handle<StandardMaterial>,
    selected: Handle<StandardMaterial>,
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
    /// Reads the rows of each of `slots`, in order: its ranks as `learning` gives them, and its
    /// charges as `book` holds them.
    fn read_slots(&mut self, slots: &ActionSlots, learning: &Learning<'_>, book: &ActionBook) {
        self.slots.clear();
        self.slots.extend(slots.iter().map(|held| SlotRows {
            ranks: learning.ranks(held),
            charges: held.charge_rule(book).map(|rule| rule.max.get()),
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

/// The roots of the drawings of a shape with no gauges yet: the unit each draws, and its shape;
/// a projectile's drawing has none, and no gauges.
type Ungauged<'w, 's> =
    Query<'w, 's, (Entity, &'static DrawingOf, &'static UnitShape), Without<Gauged>>;

/// What the gauges read of a drawn unit as they are made: its team, its life, its other pools, and
/// its ability slots.
type Gaugeable<'w, 's> = Query<
    'w,
    's,
    (
        &'static Team,
        Option<&'static Life>,
        Option<&'static Pools>,
        Option<&'static ActionSlots>,
    ),
    Allow<Unpredicted>,
>;

/// On the root of a unit's drawing, the client's own entity: its gauges are made, on their stand,
/// when it has any; and on the own avatar's, the rows of the ability slots they were made for.
#[derive(Component, Debug, Default)]
struct Gauged {
    stand: Option<Entity>,
    slots: Option<Vec<SlotRows>>,
    /// The badge of its level on its stand, of a unit with a life.
    badge: Option<Entity>,
}

/// A level's badge on a unit's stand, shown at each level the HUD file gives an icon.
#[derive(Component, Debug)]
struct LevelBadge;

/// The material of each level's icon from 2, of the HUD file's rectangles of its textures, each
/// fitted to its rectangle once its texture loads.
#[derive(Resource, Debug, Default)]
struct LevelBadges(Vec<BadgeMaterial>);

/// A level's material: its texture, the rectangle of it the icon shows, in pixels, and whether
/// its texture's coordinates fit the rectangle yet.
#[derive(Debug)]
struct BadgeMaterial {
    image: Handle<Image>,
    rect: Rect,
    material: Handle<StandardMaterial>,
    fitted: bool,
}

impl Gauged {
    /// Takes the gauges of the drawing `root` down: its stand, and its mark of gauges.
    fn take_down(&self, root: Entity, commands: &mut Commands<'_, '_>) {
        if let Some(stand) = self.stand {
            commands.entity(stand).despawn();
        }
        commands.entity(root).remove::<Self>();
    }
}

/// The gauges of each drawing's root.
type Roots<'w, 's> = Query<'w, 's, &'static Gauged>;

/// What the gauges show of each unit, and where they stand.
type Shown<'w, 's> = Query<
    'w,
    's,
    (
        &'static Drawing,
        Has<Dead>,
        (Option<&'static Life>, Option<&'static Pools>),
        (Option<&'static ActionSlots>, Option<&'static SlotTimes>),
        Option<&'static Points>,
        Option<&'static Level>,
    ),
    Allow<Unpredicted>,
>;

/// The drawn units whose gauges may show something else this frame, as what they show changed.
type Restated<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        With<Drawing>,
        Or<(
            Changed<Life>,
            Changed<Pools>,
            Changed<ActionSlots>,
            Changed<SlotTimes>,
            Changed<Points>,
            Changed<Level>,
            Added<Dead>,
        )>,
        Allow<Unpredicted>,
    ),
>;

/// The drawn units whose ability slots changed.
type Reslotted<'w, 's> = Query<
    'w,
    's,
    (&'static Drawing, &'static ActionSlots),
    (Changed<ActionSlots>, Allow<Unpredicted>),
>;

/// The drawings of the units whose team changed this frame.
type Resided<'w, 's> = Query<'w, 's, &'static Drawing, (Changed<Team>, Allow<Unpredicted>)>;

/// The units whose gauges are new this frame, by their drawings' roots.
type NewlyGauged<'w, 's> = Query<'w, 's, &'static DrawingOf, Added<Gauged>>;

/// The camera's place and projection, which turn and scale the stands.
type Eye<'w, 's> = Single<
    'w,
    's,
    (Ref<'static, Transform>, Ref<'static, Projection>),
    (With<Camera3d>, Without<GaugeStand>),
>;

/// Each drawing's root that is gauged, with its unit.
type GaugedRoots<'w, 's> = Query<'w, 's, (&'static DrawingOf, Ref<'static, Gauged>)>;

/// The gauge stands, by the gauges they hold.
type Stands<'w, 's> = Query<'w, 's, &'static Children, With<GaugeStand>>;

/// The drawn units whose life changed.
type Hurt<'w, 's> =
    Query<'w, 's, (&'static Life, &'static Drawing), (Changed<Life>, Allow<Unpredicted>)>;

/// The drawn units whose level changed.
type ChangedLevels<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        With<Drawing>,
        Or<(Changed<Level>, Added<Dead>)>,
        Allow<Unpredicted>,
    ),
>;

/// What a badge shows of each unit: its drawing, its level and whether it is dead.
type Leveled<'w, 's> =
    Query<'w, 's, (&'static Drawing, Option<&'static Level>, Has<Dead>), Allow<Unpredicted>>;

/// The badges, by their units' gauges.
type Badges<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut MeshMaterial3d<StandardMaterial>,
        &'static mut Visibility,
    ),
    (With<LevelBadge>, Without<Gauge>),
>;

/// The fill of each gauge: a drawn quad that no gauge holds.
type Fills<'w, 's> = Query<'w, 's, &'static mut Transform, With<GaugeFill>>;

/// What a ring under a unit marks: that the player selected the unit, or that the own avatar
/// attacks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Marks {
    Selected,
    Target,
}

/// A ring under a unit, a child of the root of its drawing, which the unit's despawn takes along:
/// the unit, what the ring marks, the root it stands under, and the ring.
#[derive(Debug, Clone, Copy)]
struct UnitRing {
    unit: StableId,
    marks: Marks,
    root: Entity,
    ring: Entity,
}

/// A unit and what a ring under it marks, the order rings are kept in.
type RingKey = (StableId, Marks);

/// A ring's width, as a share of its inner radius, which its scale takes to the unit's reach.
const RING_WIDTH: f32 = 0.12;

/// How far over the ground a selected unit's ring lies, over an area's disc, and a target's,
/// over that, and how much wider than its reach a target's is.
const SELECTED_LIFT: f32 = 0.035;
const TARGET_LIFT: f32 = 0.04;
const TARGET_SCALE: f32 = 1.25;

/// The texture coordinates of a badge's quad that show `rect` of its texture of `size`, its
/// pixels as shares of the texture's.
fn badge_uv(rect: Rect, size: Vec2) -> Affine2 {
    Affine2::from_scale_angle_translation(rect.size() / size, 0.0, rect.min / size)
}

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, Self::set_palette);
        app.add_systems(
            Update,
            (
                Self::take_down_torn,
                Self::regauge,
                Self::reside,
                Self::add_gauges,
                Self::show_selected_bars.run_if(Self::shows_selected),
                Self::face_camera,
                Self::mark_hits.run_if(Self::marks_hits),
                Self::fill_gauges,
                Self::load_badges
                    .run_if(resource_exists::<MatchHud>)
                    .run_if(not(resource_exists::<LevelBadges>)),
                Self::fit_badges
                    .run_if(resource_exists::<LevelBadges>)
                    .run_if(resource_exists::<Assets<Image>>),
                Self::show_levels.run_if(resource_exists::<LevelBadges>),
                Self::widen_rings,
                Self::mark_rings,
            )
                .chain()
                .after(ViewSystems)
                .after(SelectionSystems),
        );
    }
}

impl HudPlugin {
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
            ring: meshes.add(Annulus::new(0.8, 1.0).mesh()),
            unit_ring: meshes.add(Annulus::new(1.0, 1.0 + RING_WIDTH).mesh()),
            back: flat(Color::srgb(0.1, 0.1, 0.12)),
            friend: flat(Color::srgb(0.3, 0.85, 0.35)),
            foe: flat(Color::srgb(0.9, 0.3, 0.25)),
            resource: flat(Color::srgb(0.3, 0.55, 1.0)),
            cooldown: flat(Color::srgb(0.9, 0.9, 0.9)),
            learnable: flat(Color::srgb(1.0, 0.72, 0.1)),
            hit: flat(Color::srgb(1.0, 0.8, 0.3)),
            target: flat(Color::srgb(1.0, 0.35, 0.2)),
            selected: flat(Color::srgb(0.3, 0.95, 0.4)),
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
        (learning, book): (Learning<'_>, Res<'_, ActionBook>),
        own: OwnPlayer<'_, '_>,
        (roots, units): (Ungauged<'_, '_>, Gaugeable<'_, '_>),
        mut scratch: Local<'_, GaugeScratch>,
        mut commands: Commands<'_, '_>,
    ) {
        if own.side().is_none() {
            return;
        }
        let avatar = own.avatar().map(|avatar| avatar.entity);
        for (root, &DrawingOf(unit), shape) in &roots {
            let Ok((team, life, pools, slots)) = units.get(unit) else {
                continue;
            };
            let mine = Some(unit) == avatar;
            let allegiance = match own.relation_to(*team) {
                Some(Relation::Friendly) => Allegiance::Own,
                Some(Relation::Neutral | Relation::Hostile) | None => Allegiance::Other,
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
                scratch.read_slots(slots, &learning, &book);
                scratch.ability_rows(row);
                Some(scratch.slots.clone())
            } else {
                None
            };
            if scratch.kinds.is_empty() {
                commands.entity(root).insert(Gauged {
                    stand: None,
                    slots: rows,
                    badge: None,
                });
                continue;
            }
            let top = Vec3::Y * shape.height;
            let stand = commands
                .spawn((
                    GaugeStand,
                    Transform::from_translation(top),
                    Visibility::Inherited,
                ))
                .id();
            let badge = life.is_some().then(|| {
                let badge = commands
                    .spawn((
                        LevelBadge,
                        Mesh3d(palette.quad.clone()),
                        MeshMaterial3d(palette.back.clone()),
                        Layout::badge(),
                        Visibility::Hidden,
                    ))
                    .id();
                commands.entity(stand).add_child(badge);
                badge
            });
            commands.entity(root).add_child(stand).insert(Gauged {
                stand: Some(stand),
                slots: rows,
                badge,
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
                        GaugeFill,
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
        (learning, book): (Learning<'_>, Res<'_, ActionBook>),
        avatars: Reslotted<'_, '_>,
        roots: Roots<'_, '_>,
        mut scratch: Local<'_, GaugeScratch>,
        mut commands: Commands<'_, '_>,
    ) {
        for (drawing, slots) in &avatars {
            let Ok(gauged) = roots.get(drawing.root()) else {
                continue;
            };
            let Some(rows) = &gauged.slots else {
                continue;
            };
            scratch.read_slots(slots, &learning, &book);
            if scratch.slots == *rows {
                continue;
            }
            gauged.take_down(drawing.root(), &mut commands);
        }
    }

    /// Takes down the gauges of each unit whose team changed, and of every unit once the teams'
    /// relations change, so `add_gauges` makes them again in the colors of how the client's side
    /// regards each unit now.
    fn reside(
        (moved, relations): (Resided<'_, '_>, Query<'_, '_, (), Changed<Relations>>),
        (gauged, roots): (Query<'_, '_, Entity, With<Gauged>>, Roots<'_, '_>),
        mut commands: Commands<'_, '_>,
    ) {
        let mut take_down = |root: Entity| {
            if let Ok(gauged) = roots.get(root) {
                gauged.take_down(root, &mut commands);
            }
        };
        if relations.is_empty() {
            for drawing in &moved {
                take_down(drawing.root());
            }
        } else {
            for root in &gauged {
                take_down(root);
            }
        }
    }

    /// Takes down the gauges of each drawing torn off its unit, as an aircraft's falls as a wreck
    /// of its own (02-engine-core.md, An aircraft's death falls as a wreck).
    fn take_down_torn(
        mut torn: RemovedComponents<'_, '_, DrawingOf>,
        roots: Roots<'_, '_>,
        mut commands: Commands<'_, '_>,
    ) {
        for root in torn.read() {
            if let Ok(gauged) = roots.get(root) {
                gauged.take_down(root, &mut commands);
            }
        }
    }

    /// Whether the mode's HUD file shows the bars of the selected and the pointed units alone.
    fn shows_selected(hud: Res<'_, MatchHud>) -> bool {
        hud.0.bars == ShownBars::Selected
    }

    /// Whether the mode's HUD file marks a hit with a ring (02-engine-core.md, The mode chooses its
    /// hit mark).
    fn marks_hits(hud: Res<'_, MatchHud>) -> bool {
        hud.0.hit_mark == HitMark::Ring
    }

    /// Shows the bars of the units the player selected and of the one under the cursor alone,
    /// and hides the others' (02-engine-core.md, Bars in screen space). A stand shows or hides
    /// again only as the selection or the pointed unit changes, or as it is new.
    fn show_selected_bars(
        (selection, pointer): (Res<'_, Selection>, Pointer<'_, '_>),
        (roots, units): (
            GaugedRoots<'_, '_>,
            Query<'_, '_, &StableId, Allow<Unpredicted>>,
        ),
        mut stands: Query<'_, '_, &mut Visibility, With<GaugeStand>>,
        mut pointed: Local<'_, Option<StableId>>,
    ) {
        let now = pointer.unit_under(&[]).map(|unit| unit.id);
        let changed = selection.is_changed() || now != *pointed;
        *pointed = now;
        for (&DrawingOf(unit), gauged) in &roots {
            if !changed && !gauged.is_changed() {
                continue;
            }
            let (Some(stand), Ok(&id)) = (gauged.stand, units.get(unit)) else {
                continue;
            };
            let shown = now == Some(id) || selection.get().binary_search(&id).is_ok();
            let visibility = if shown {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if let Ok(mut place) = stands.get_mut(stand) {
                place.set_if_neq(visibility);
            }
        }
    }

    /// Turns each gauge stand to face the camera in the world, its up, the gauges' face, toward
    /// the camera's back, its right along the camera's, and scales it so one of its units is a
    /// pixel of the screen at its depth, as an RTS draws its bars (02-engine-core.md, Bars in
    /// screen space). A stand turns in its drawing's root, which turns as its unit faces, so its
    /// own turn takes the root's away. A stand changes again only as the camera, its projection or
    /// the window changes, its root moves, or it is new.
    fn face_camera(
        camera: Eye<'_, '_>,
        window: Single<'_, '_, Ref<'_, Window>, With<PrimaryWindow>>,
        roots: Query<'_, '_, Ref<'_, Transform>, (Without<GaugeStand>, Without<Camera3d>)>,
        mut stands: Query<'_, '_, (&ChildOf, Ref<'_, GaugeStand>, &mut Transform)>,
    ) {
        let (eye, projection) = camera.into_inner();
        let facing = eye.rotation * Quat::from_rotation_x(FRAC_PI_2);
        let moved = eye.is_changed() || projection.is_changed() || window.is_changed();
        let pixels = PixelScale::of(*eye, &projection, window.height());
        for (&ChildOf(root), stand, mut place) in &mut stands {
            let Ok(root) = roots.get(root) else {
                continue;
            };
            if !moved && !root.is_changed() && !stand.is_added() {
                continue;
            }
            let rotation = root.rotation.inverse() * facing;
            let point = root.translation + root.rotation * place.translation;
            let scale = pixels.meters_at(point).map_or(place.scale, Vec3::splat);
            if place.rotation != rotation || place.scale != scale {
                place.rotation = rotation;
                place.scale = scale;
            }
        }
    }

    /// Puts a ring on the ground where a unit's life dropped since its gauge last showed it, as
    /// wide as the unit's reach.
    fn mark_hits(
        time: Res<'_, Time>,
        palette: Res<'_, HudPalette>,
        (units, gauged): (Hurt<'_, '_>, Roots<'_, '_>),
        roots: Query<'_, '_, (&Transform, &UnitShape), Without<GaugeStand>>,
        stands: Stands<'_, '_>,
        mut gauges: Query<'_, '_, &mut Gauge>,
        mut commands: Commands<'_, '_>,
    ) {
        for (life, drawing) in &units {
            let stand = gauged
                .get(drawing.root())
                .ok()
                .and_then(|gauged| gauged.stand);
            let (Some(stand), Ok((root, shape))) = (stand, roots.get(drawing.root())) else {
                continue;
            };
            let Ok(children) = stands.get(stand) else {
                continue;
            };
            let mut gauges = gauges.iter_many_mut(children);
            while let Some(gauge) = gauges.fetch_next() {
                let Ok(mut gauge) = gauge else {
                    continue;
                };
                let GaugeKind::Life { shown } = &mut gauge.kind else {
                    continue;
                };
                let current = life.current();
                if shown.is_some_and(|shown| current < shown) {
                    commands.spawn((
                        Mesh3d(palette.ring.clone()),
                        MeshMaterial3d(palette.hit.clone()),
                        Transform::from_translation(root.translation + Vec3::Y * 0.05)
                            .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
                            .with_scale(Vec3::splat(shape.reach())),
                        Ring {
                            since: time.elapsed(),
                            reach: shape.reach(),
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
        tick: Res<'_, SimTick>,
        own: OwnPlayer<'_, '_>,
        (learning, book): (Learning<'_>, Res<'_, ActionBook>),
        (restated, newly, gauged): (Restated<'_, '_>, NewlyGauged<'_, '_>, Roots<'_, '_>),
        mut revived: RemovedComponents<'_, '_, Dead>,
        units: Shown<'_, '_>,
        stands: Stands<'_, '_>,
        (mut gauges, mut fills): (Query<'_, '_, (&mut Gauge, &mut Visibility)>, Fills<'_, '_>),
        mut touched: Local<'_, Vec<Entity>>,
    ) {
        let avatar = own.avatar().map(|avatar| avatar.entity);
        touched.clear();
        let new = newly.iter().map(|&DrawingOf(unit)| unit);
        touched.extend(
            restated
                .iter()
                .chain(new)
                .chain(revived.read())
                .chain(avatar),
        );
        touched.sort_unstable();
        touched.dedup();
        let now = tick.start();
        for &unit in &*touched {
            let Ok((drawing, dead, (life, pools), (slots, times), points, level)) = units.get(unit)
            else {
                continue;
            };
            let stand = gauged
                .get(drawing.root())
                .ok()
                .and_then(|gauged| gauged.stand);
            let Some(Ok(children)) = stand.map(|stand| stands.get(stand)) else {
                continue;
            };
            let mut gauges = gauges.iter_many_mut(children);
            while let Some(gauge) = gauges.fetch_next() {
                let Ok((mut gauge, mut visibility)) = gauge else {
                    continue;
                };
                let fill = |pool| {
                    let pools = pools?;
                    Some(Gauge::share(pools.current(pool)?, pools.max(pool)?))
                };
                let fraction =
                    match &mut gauge.kind {
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
                            time.map(|time| {
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
                            let rule = held.and_then(|held| held.charge_rule(&book));
                            let time = times.and_then(|times| times.slot(*slot));
                            time.zip(rule).map(|(time, rule)| {
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
                            let learnable = held.zip(points.zip(level)).is_some_and(
                                |(held, (&points, &level))| learning.learnable(held, points, level),
                            );
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

    /// Makes the material of each level's icon of the match's HUD file, its texture of the mode's
    /// package.
    fn load_badges(
        hud: Res<'_, MatchHud>,
        assets: Res<'_, AssetServer>,
        mut materials: ResMut<'_, Assets<StandardMaterial>>,
        mut commands: Commands<'_, '_>,
    ) {
        let badges = hud.0.level_icons.iter().map(|icon| {
            let path = PackageAssets::asset_path(PackageIndex::MODE, &icon.texture);
            let image: Handle<Image> = assets.load(AssetPath::from(path));
            let [left, top, right, bottom] = icon.rect.edges().map(f32::from);
            BadgeMaterial {
                material: materials.add(StandardMaterial {
                    base_color_texture: Some(image.clone()),
                    unlit: true,
                    alpha_mode: AlphaMode::Blend,
                    ..StandardMaterial::default()
                }),
                image,
                rect: Rect::new(left, top, right, bottom),
                fitted: false,
            }
        });
        commands.insert_resource(LevelBadges(badges.collect()));
    }

    /// Fits each level's material to its icon's rectangle once its texture loads, as a
    /// rectangle's share of its texture's size.
    fn fit_badges(
        mut badges: ResMut<'_, LevelBadges>,
        images: Res<'_, Assets<Image>>,
        mut materials: ResMut<'_, Assets<StandardMaterial>>,
    ) {
        for badge in badges.0.iter_mut().filter(|badge| !badge.fitted) {
            let Some(image) = images.get(&badge.image) else {
                continue;
            };
            let size = image.size_f32();
            if let Some(mut material) = materials.get_mut(&badge.material) {
                material.uv_transform = badge_uv(badge.rect, size);
            }
            badge.fitted = true;
        }
    }

    /// Shows the badge of each drawn unit's level that changed, or whose gauges are new: the icon
    /// of its level, none at level 1 or past the HUD file's last icon, nor while it is dead.
    fn show_levels(
        badges: Res<'_, LevelBadges>,
        (changed, newly, gauged): (ChangedLevels<'_, '_>, NewlyGauged<'_, '_>, Roots<'_, '_>),
        units: Leveled<'_, '_>,
        mut shown: Badges<'_, '_>,
    ) {
        let new = newly.iter().map(|&DrawingOf(unit)| unit);
        for unit in changed.iter().chain(new) {
            let Ok((drawing, level, dead)) = units.get(unit) else {
                continue;
            };
            let badge = gauged
                .get(drawing.root())
                .ok()
                .and_then(|gauged| gauged.badge);
            let Some(Ok((mut material, mut visibility))) = badge.map(|badge| shown.get_mut(badge))
            else {
                continue;
            };
            let at = level.map_or(1, |level| level.get());
            let icon = usize::try_from(at)
                .ok()
                .and_then(|at| at.checked_sub(2))
                .and_then(|at| badges.0.get(at))
                .filter(|_| !dead);
            match icon {
                Some(icon) => {
                    if material.0 != icon.material {
                        material.0 = icon.material.clone();
                    }
                    visibility.set_if_neq(Visibility::Inherited);
                }
                None => {
                    visibility.set_if_neq(Visibility::Hidden);
                }
            }
        }
    }

    /// Puts a ring under each unit the player selected, at its reach, under each living spawn of a
    /// selected leader in its place (02-engine-core.md, A member picks as its leader), and a wider
    /// one under the unit the own avatar attacks, once each one's drawing has its shape, and again
    /// under a new drawing of it; takes each ring away as its unit leaves the selection or the
    /// attack.
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    fn mark_rings(
        (selection, index, leaders): (
            Res<'_, Selection>,
            Res<'_, EntityIndex>,
            Query<'_, '_, &LivingSpawns, With<Leader>>,
        ),
        palette: Res<'_, HudPalette>,
        own: OwnPlayer<'_, '_>,
        slots: Query<'_, '_, &ActionProgress>,
        units: Query<'_, '_, &Drawing, Allow<Unpredicted>>,
        shapes: Query<'_, '_, &UnitShape>,
        (mut rings, mut wanted): (Local<'_, Vec<UnitRing>>, Local<'_, Vec<RingKey>>),
        mut commands: Commands<'_, '_>,
    ) {
        wanted.clear();
        for &unit in selection.get() {
            let led = index.get(unit).and_then(|unit| leaders.get(unit).ok());
            match led {
                Some(spawns) => wanted.extend(spawns.iter().map(|&spawn| (spawn, Marks::Selected))),
                None => wanted.push((unit, Marks::Selected)),
            }
        }
        let target = own
            .avatar()
            .and_then(|avatar| slots.get(avatar.entity).ok())
            .and_then(ActionProgress::attack_target);
        wanted.extend(target.map(|target| (target, Marks::Target)));
        wanted.sort_unstable();
        wanted.dedup();
        let root_of = |unit| {
            let root = index
                .get(unit)
                .and_then(|unit| units.get(unit).ok())?
                .root();
            Some((root, shapes.get(root).ok()?))
        };
        rings.retain(|ring| {
            let wants = wanted.binary_search(&(ring.unit, ring.marks)).is_ok();
            let under = root_of(ring.unit).is_some_and(|(root, _)| root == ring.root);
            if !(wants && under) {
                commands.entity(ring.ring).try_despawn();
            }
            wants && under
        });
        let before = rings.len();
        for &(unit, marks) in &*wanted {
            let held = rings[..before]
                .binary_search_by_key(&(unit, marks), |ring| (ring.unit, ring.marks));
            if held.is_ok() {
                continue;
            }
            let Some((root, shape)) = root_of(unit) else {
                continue;
            };
            let (material, scale, lift) = match marks {
                Marks::Selected => (&palette.selected, shape.reach(), SELECTED_LIFT),
                Marks::Target => (&palette.target, TARGET_SCALE * shape.reach(), TARGET_LIFT),
            };
            let ring = commands
                .spawn((
                    Mesh3d(palette.unit_ring.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::from_translation(Vec3::Y * lift)
                        .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
                        .with_scale(Vec3::splat(scale)),
                    ChildOf(root),
                ))
                .id();
            rings.push(UnitRing {
                unit,
                marks,
                root,
                ring,
            });
        }
        rings.sort_unstable_by_key(|ring| (ring.unit, ring.marks));
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
mod tests;
