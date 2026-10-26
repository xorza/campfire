use bevy::app::{App, Plugin, Startup, Update};
use bevy::asset::{AssetPath, AssetServer, Assets, Handle, RenderAssetUsages};
use bevy::camera::Camera3d;
use bevy::camera::visibility::{NoFrustumCulling, Visibility};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::lifecycle::RemovedComponents;
use bevy::ecs::name::Name;
use bevy::ecs::query::With;
use bevy::ecs::query::{Added, Allow, Changed};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut, Single};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::math::Affine3A;
use bevy::mesh::{Indices, Mesh, Mesh3d, PrimitiveTopology, VertexAttributeValues};
use bevy::pbr::MeshMaterial3d;
use bevy::picking::Pickable;
use bevy::time::Time;
use bevy::transform::components::GlobalTransform;
use bevy::transform::components::Transform;
use campfire_actions::{ActionProgress, ActionSlots, SlotTimes};
use campfire_common::Tick;
use campfire_projectiles::Projectile;
use campfire_script::MatchNames;
use campfire_sim::{EntityIndex, Unpredicted};
use campfire_units::{Dead, TypeOriginBook};
use campfire_values::{PackageIndex, UnitType};

use crate::view::client_data::ClientLoad;
use crate::view::drawing::Drawing;
use crate::view::effects::effect_book::{EffectBook, GroupId};
use crate::view::effects::effect_plays::{EffectPlays, PlaySources};
use crate::view::effects::particle_quads::{ParticleQuads, ViewAxes};
use crate::view::effects::particle_run::{ParticleRun, SystemKey};
use crate::view::effects::step_clock::StepClock;
use crate::view::file_material::{FileBlend, FileMaterial};
use crate::view::ground_heights::GroundHeights;
use crate::view::look_states::LookStates;
use crate::view::package_source::PackageAssets;
use crate::view::unit_looks::UnitLooks;
use crate::view::{DrawSystems, ViewSystems};

pub(crate) mod effect_book;
pub(crate) mod effect_plays;
pub(crate) mod particle_quads;
pub(crate) mod particle_run;
pub(crate) mod step_clock;

/// The match's effects, of the mode's own package's effects file: its book, the particle systems
/// alive, the clock their steps keep, the frame's quads, and each draw group's mesh and its
/// entity's visibility, hidden while it holds no quad.
#[derive(Resource, Debug)]
pub(crate) struct MatchEffects {
    pub(crate) book: EffectBook,
    pub(crate) run: ParticleRun,
    plays: EffectPlays,
    clock: StepClock,
    quads: ParticleQuads,
    meshes: Vec<GroupMesh>,
    flights: Vec<Flight>,
    followers: Vec<Follower>,
}

/// A projectile in flight: its drawing's root, its type, the trail it carries, and where its
/// drawing stood last, where it ends once its drawing goes.
#[derive(Debug)]
struct Flight {
    root: Entity,
    unit_type: UnitType,
    trail: Option<SystemKey>,
    at: Affine3A,
}

/// A system an effect started on its object, which follows the object's drawing until it goes.
#[derive(Debug)]
struct Follower {
    system: SystemKey,
    root: Entity,
}

/// On a unit's drawing's root: the latest tick the effects saw each slot of its unit ready at, a
/// rise past which is a shot (02-engine-core.md, The client derives an attack's phase), a
/// rollback that sets a slot back and fires it again playing no second shot; and the barrel each
/// weapon slot of its look fires its next shot from, as Zero Hour turns a weapon's barrel with
/// each shot (effects.md, A shot's effect at its barrel).
#[derive(Component, Debug, Default)]
pub(crate) struct FireSeen {
    ready: Vec<Tick>,
    turns: Vec<usize>,
}

impl FireSeen {
    /// The barrel of `barrels` the weapon slot `slot` fires from, and turns it to the next, back
    /// to the first after the last.
    fn turn(&mut self, slot: usize, barrels: usize) -> usize {
        if self.turns.len() <= slot {
            self.turns.resize(slot + 1, 0);
        }
        let barrel = self.turns[slot] % barrels;
        self.turns[slot] = (barrel + 1) % barrels;
        barrel
    }
}

/// The units that died, of their types and drawings.
type Dying<'w, 's> =
    Query<'w, 's, (Entity, &'static UnitType, &'static Drawing), (Added<Dead>, Allow<Unpredicted>)>;

/// The projectiles drawn of late, of their types and drawings.
type NewShots<'w, 's> = Query<
    'w,
    's,
    (&'static UnitType, &'static Drawing),
    (With<Projectile>, Added<Drawing>, Allow<Unpredicted>),
>;

/// The units whose slots' times changed, of their types, slots, attacks and drawings.
type Shooters<'w, 's> = Query<
    'w,
    's,
    (
        &'static UnitType,
        &'static SlotTimes,
        &'static ActionSlots,
        &'static ActionProgress,
        &'static Drawing,
    ),
    (Changed<SlotTimes>, Allow<Unpredicted>),
>;

/// A draw group's mesh, its entity, and whether the entity shows, by its place among the
/// groups.
#[derive(Debug)]
struct GroupMesh {
    mesh: Handle<Mesh>,
    entity: Entity,
    shown: bool,
}

/// Steps and draws the match's effects (effects.md, Particles run on the CPU and draw as
/// billboards of the one material).
#[derive(Debug)]
pub(crate) struct EffectsPlugin;

/// The draws a particle run makes from, the same in every match.
const SEED: u64 = 0x5EED_5EED_5EED_5EED;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, MatchEffects::load);
        app.add_systems(
            Update,
            (
                MatchEffects::play_fires,
                MatchEffects::play_deaths,
                MatchEffects::fly,
                MatchEffects::follow,
                MatchEffects::step,
                MatchEffects::draw,
            )
                .chain()
                .after(DrawSystems)
                .in_set(ViewSystems),
        );
    }
}

impl MatchEffects {
    /// Reads the mode's effects, and gives each draw group its material, of its texture, and an
    /// empty mesh to draw in; none for a mode of no effects.
    fn load(
        (load, origins, names): (
            Res<'_, ClientLoad>,
            Res<'_, TypeOriginBook>,
            Res<'_, MatchNames>,
        ),
        assets: Res<'_, AssetServer>,
        (mut meshes, mut materials): (ResMut<'_, Assets<Mesh>>, ResMut<'_, Assets<FileMaterial>>),
        mut commands: Commands<'_, '_>,
    ) {
        let Some(file) = &load.effects else {
            return;
        };
        let book = EffectBook::new(file);
        let looks = load.packages.first().map(|package| &package.units);
        let plays = looks.map_or_else(EffectPlays::default, |looks| {
            EffectPlays::of(PlaySources {
                book: &book,
                looks,
                origins: &origins,
                names: &names,
            })
        });
        let meshes = book
            .groups()
            .iter()
            .map(|group| {
                let path = PackageAssets::asset_path(PackageIndex::MODE, &group.texture);
                let texture = assets.load(AssetPath::from(path));
                let material = materials.add(FileBlend::particles(texture, group.blend));
                let mesh = meshes.add(Self::empty());
                let entity = commands
                    .spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(material),
                        Transform::IDENTITY,
                        Visibility::Hidden,
                        NoFrustumCulling,
                        NotShadowCaster,
                        NotShadowReceiver,
                        Pickable::IGNORE,
                    ))
                    .id();
                GroupMesh {
                    mesh,
                    entity,
                    shown: false,
                }
            })
            .collect();
        commands.insert_resource(Self {
            book,
            run: ParticleRun::new(SEED),
            plays,
            clock: StepClock::default(),
            quads: ParticleQuads::default(),
            meshes,
            flights: Vec::new(),
            followers: Vec::new(),
        });
    }

    /// Plays each shot's fire effect at its unit's barrel, the bone its type's state names for
    /// the shot's weapon slot, or at its unit's drawing for none; and each shot's hit of a weapon
    /// of no projectile at its target's drawing (effects.md, A shot's effect at its barrel). A
    /// shot is a rise of a slot's ready tick, of a unit whose slots' times changed.
    fn play_fires(
        effects: Option<ResMut<'_, Self>>,
        (looks, index, heights): (
            Res<'_, UnitLooks>,
            Res<'_, EntityIndex>,
            Option<Res<'_, GroundHeights>>,
        ),
        shooters: Shooters<'_, '_>,
        drawn: Query<'_, '_, &Drawing, Allow<Unpredicted>>,
        (mut seen, states): (Query<'_, '_, &mut FireSeen>, Query<'_, '_, &LookStates>),
        (places, children, names): (
            Query<'_, '_, &GlobalTransform>,
            Query<'_, '_, &Children>,
            Query<'_, '_, &Name>,
        ),
        mut commands: Commands<'_, '_>,
    ) {
        let Some(mut effects) = effects else {
            return;
        };
        let effects = &mut *effects;
        let ground = Self::ground(heights.as_deref());
        for (&unit_type, times, slots, progress, drawing) in &shooters {
            let root = drawing.root();
            let Ok(mut fired) = seen.get_mut(root) else {
                commands.entity(root).insert(FireSeen {
                    ready: times.iter().map(|time| time.ready_at).collect(),
                    turns: Vec::new(),
                });
                continue;
            };
            let fresh = fired.ready.len() != times.iter().len();
            for (at, (time, slot)) in times.iter().zip(slots.iter()).enumerate() {
                let rose = fired
                    .ready
                    .get(at)
                    .is_some_and(|held| time.ready_at > *held);
                let Some(action) = slot.action.filter(|_| rose && !fresh) else {
                    continue;
                };
                if let Some(&fire) = effects.plays.fires.get(&action) {
                    let group = looks
                        .weapons(unit_type)
                        .iter()
                        .position(|group| group.contains(&action));
                    let barrel = group.and_then(|group| {
                        let slot = u8::try_from(group).ok()?;
                        let barrels = states.get(root).ok()?.barrels(&looks, slot)?;
                        Some(&barrels[fired.turn(group, barrels.len())])
                    });
                    // A barrel of no fire bone plays no fire effect, as the game's plays none.
                    let plays = barrel.is_none_or(|barrel| barrel.bone.is_some());
                    let bone = barrel.and_then(|barrel| barrel.bone.as_deref());
                    let barrel = bone
                        .and_then(|bone| Self::bone(root, bone, &children, &names))
                        .unwrap_or(root);
                    if let Some(place) = places.get(barrel).ok().filter(|_| plays) {
                        effects
                            .run
                            .play(&effects.book, fire, place.affine(), &ground);
                        effects.follow_last(root);
                    }
                }
                if let Some(&hit) = effects.plays.hits.get(&action) {
                    let target = progress.attacking().and_then(|target| index.get(target));
                    let target = target.and_then(|target| drawn.get(target).ok());
                    if let Some(place) = target.and_then(|drawing| places.get(drawing.root()).ok())
                    {
                        effects
                            .run
                            .play(&effects.book, hit, place.affine(), &ground);
                    }
                }
            }
            if fresh {
                fired.ready.clear();
                fired.ready.extend(times.iter().map(|time| time.ready_at));
            } else {
                for (held, time) in fired.ready.iter_mut().zip(times.iter()) {
                    *held = (*held).max(time.ready_at);
                }
            }
        }
    }

    /// Plays each unit type's death effect at its unit's drawing as the unit dies; a rollback
    /// across a predicted death, which takes `Dead` away and gives it back in one frame, plays
    /// none again.
    fn play_deaths(
        effects: Option<ResMut<'_, Self>>,
        heights: Option<Res<'_, GroundHeights>>,
        (dying, mut revived): (Dying<'_, '_>, RemovedComponents<'_, '_, Dead>),
        places: Query<'_, '_, &GlobalTransform>,
        mut back: Local<'_, Vec<Entity>>,
    ) {
        back.clear();
        back.extend(revived.read());
        back.sort_unstable();
        let Some(mut effects) = effects else {
            return;
        };
        let effects = &mut *effects;
        let ground = Self::ground(heights.as_deref());
        for (unit, unit_type, drawing) in &dying {
            if back.binary_search(&unit).is_ok() {
                continue;
            }
            let death = effects.plays.deaths.get(unit_type);
            let (Some(&death), Ok(place)) = (death, places.get(drawing.root())) else {
                continue;
            };
            effects
                .run
                .play(&effects.book, death, place.affine(), &ground);
            effects.follow_last(drawing.root());
        }
    }

    /// Starts each new projectile's trail on its drawing, moves each trail along with its
    /// projectile's drawing, and plays a projectile's end effect where its drawing stood last
    /// once it goes, its trail stopped. A drawing's root stands in the world, so its transform is
    /// its place already in the frame it is spawned, before its global transform is.
    fn fly(
        effects: Option<ResMut<'_, Self>>,
        heights: Option<Res<'_, GroundHeights>>,
        shots: NewShots<'_, '_>,
        roots: Query<'_, '_, &Transform>,
    ) {
        let Some(mut effects) = effects else {
            return;
        };
        let effects = &mut *effects;
        for (&unit_type, drawing) in &shots {
            let root = drawing.root();
            let Ok(place) = roots.get(root) else {
                continue;
            };
            let at = place.compute_affine();
            let trail = effects.plays.trails.get(&unit_type);
            let trail = trail.map(|&trail| effects.run.start(&effects.book, trail, at));
            if trail.is_some() || effects.plays.ends.contains_key(&unit_type) {
                effects.flights.push(Flight {
                    root,
                    unit_type,
                    trail,
                    at,
                });
            }
        }
        let ground = Self::ground(heights.as_deref());
        let Self {
            book,
            run,
            plays,
            flights,
            ..
        } = effects;
        flights.retain_mut(|flight| {
            if let Ok(place) = roots.get(flight.root) {
                flight.at = place.compute_affine();
                if let Some(trail) = flight.trail {
                    run.place(trail, flight.at);
                }
                return true;
            }
            if let Some(trail) = flight.trail {
                run.destroy(trail);
            }
            if let Some(&end) = plays.ends.get(&flight.unit_type) {
                run.play(book, end, flight.at, &ground);
            }
            false
        });
    }

    /// Moves each system an effect started on its object with the object's drawing, and stops
    /// it once the drawing goes.
    fn follow(effects: Option<ResMut<'_, Self>>, places: Query<'_, '_, &GlobalTransform>) {
        let Some(mut effects) = effects else {
            return;
        };
        let Self { run, followers, .. } = &mut *effects;
        followers.retain(|follower| {
            if let Ok(place) = places.get(follower.root) {
                run.place(follower.system, place.affine());
                true
            } else {
                run.destroy(follower.system);
                false
            }
        });
    }

    /// Keeps the systems the last play started on its object following `root`.
    fn follow_last(&mut self, root: Entity) {
        let attached = self.run.attached().iter();
        self.followers
            .extend(attached.map(|&system| Follower { system, root }));
    }

    /// The node of the name `bone` among `root` and its descendants, the first found depth first.
    fn bone(
        root: Entity,
        bone: &str,
        children: &Query<'_, '_, &Children>,
        names: &Query<'_, '_, &Name>,
    ) -> Option<Entity> {
        let mut open = vec![root];
        while let Some(at) = open.pop() {
            if names.get(at).is_ok_and(|name| name.as_str() == bone) {
                return Some(at);
            }
            if let Ok(below) = children.get(at) {
                open.extend(below.iter());
            }
        }
        None
    }

    /// The ground's height under a point's `x` and `z`, of the map's heights, 0 on a map of
    /// none.
    fn ground(heights: Option<&GroundHeights>) -> impl Fn(f32, f32) -> f32 + '_ {
        move |x, z| heights.map_or(0.0, |heights| heights.at(x, z))
    }

    /// Runs the steps the frame's time owes, each of the book's length, over the ground's
    /// heights, or level ground at 0 on a map of none.
    fn step(
        time: Res<'_, Time>,
        effects: Option<ResMut<'_, Self>>,
        heights: Option<Res<'_, GroundHeights>>,
    ) {
        let Some(mut effects) = effects else {
            return;
        };
        let effects = &mut *effects;
        let steps = effects
            .clock
            .take(time.delta_secs_f64(), effects.book.step());
        let ground = Self::ground(heights.as_deref());
        for _ in 0..steps {
            effects.run.step(&effects.book, &ground);
        }
    }

    /// Writes each draw group's mesh of its particles' quads, facing the camera, and shows the
    /// meshes that hold any.
    fn draw(
        effects: Option<ResMut<'_, Self>>,
        camera: Single<'_, '_, &Transform, With<Camera3d>>,
        mut meshes: ResMut<'_, Assets<Mesh>>,
        mut commands: Commands<'_, '_>,
    ) {
        let Some(mut effects) = effects else {
            return;
        };
        let effects = &mut *effects;
        let view = ViewAxes {
            right: camera.right().into(),
            up: camera.up().into(),
        };
        effects.quads.build(&effects.book, &effects.run, view);
        for (at, group) in effects.meshes.iter_mut().enumerate() {
            let quads = effects.quads.group(GroupId::of(at));
            let shown = !quads.indices.is_empty();
            if shown != group.shown {
                group.shown = shown;
                let visibility = if shown {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                commands.entity(group.entity).insert(visibility);
            }
            if !shown {
                continue;
            }
            let Some(mut mesh) = meshes.get_mut(&group.mesh) else {
                continue;
            };
            if let Some(VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
            {
                positions.clear();
                positions.extend_from_slice(&quads.positions);
            }
            if let Some(VertexAttributeValues::Float32x2(uvs)) =
                mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0)
            {
                uvs.clear();
                uvs.extend_from_slice(&quads.uvs);
            }
            if let Some(VertexAttributeValues::Float32x4(colors)) =
                mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
            {
                colors.clear();
                colors.extend_from_slice(&quads.colors);
            }
            if let Some(Indices::U32(indices)) = mesh.indices_mut() {
                indices.clear();
                indices.extend_from_slice(&quads.indices);
            }
        }
    }

    /// A mesh of no quad, of the attributes a quad writes.
    fn empty() -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, Vec::<[f32; 4]>::new())
        .with_inserted_indices(Indices::U32(Vec::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_turns_to_its_next_barrel_with_each_shot_and_back_to_the_first() {
        // Slot 1 of two barrels: 0, 1, 0; slot 0 of three, turned on its own: 0, 1, 2, 0.
        let mut seen = FireSeen::default();
        let slot_one: Vec<usize> = (0..3).map(|_| seen.turn(1, 2)).collect();
        assert_eq!(slot_one, [0, 1, 0]);
        let slot_zero: Vec<usize> = (0..4).map(|_| seen.turn(0, 3)).collect();
        assert_eq!(slot_zero, [0, 1, 2, 0]);
        // A state of fewer barrels takes the turn within its own.
        assert_eq!(seen.turn(0, 1), 0);
    }
}
