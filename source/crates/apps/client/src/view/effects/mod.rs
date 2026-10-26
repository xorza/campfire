use bevy::app::{App, FixedPostUpdate, Plugin, Startup, Update};
use bevy::asset::{AssetPath, AssetServer, Assets, Handle, RenderAssetUsages};
use bevy::camera::Camera3d;
use bevy::camera::visibility::{NoFrustumCulling, Visibility};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::lifecycle::RemovedComponents;
use bevy::ecs::name::Name;
use bevy::ecs::query::With;
use bevy::ecs::query::{Added, Allow, Has};
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
use campfire_actions::ActionProgress;
use campfire_combat::OnDeath;
use campfire_declarations::ById;
use campfire_projectiles::Projectile;
use campfire_script::MatchNames;
use campfire_sim::{EntityIndex, MatchRate, Unpredicted};
use campfire_units::{Dead, TypeOriginBook};
use campfire_values::{PackageIndex, UnitType};

use crate::view::client_data::ClientLoad;
use crate::view::drawing::Drawing;
use crate::view::effects::effect_book::{EffectBook, GroupId};
use crate::view::effects::effect_plays::{EffectPlays, PlaySources};
use crate::view::effects::fire_seen::{FireSeen, SeenShots};
use crate::view::effects::muzzle_flashes::{FlashParts, MuzzleFlashes};
use crate::view::effects::particle_quads::{GroupQuads, ParticleQuads, ViewAxes};
use crate::view::effects::particle_run::{ParticleRun, SystemKey};
use crate::view::effects::seen_deaths::{DeathStep, DeathUnderWay, SeenDeaths, UnitNow};
use crate::view::effects::step_clock::StepClock;
use crate::view::file_material::{FileBlend, FileMaterial, FileMaterialPlugin};
use crate::view::ground_heights::GroundHeights;
use crate::view::look_states::LookStates;
use crate::view::package_source::PackageAssets;
use crate::view::unit_looks::UnitLooks;
use crate::view::unit_models::DressSystems;
use crate::view::{DrawSystems, ViewSystems};

pub(crate) mod effect_book;
pub(crate) mod effect_plays;
pub(crate) mod fire_seen;
pub(crate) mod muzzle_flashes;
pub(crate) mod particle_quads;
pub(crate) mod particle_run;
pub(crate) mod seen_deaths;
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

/// What a death's phases are played by: the drawn ground's heights, the app's clock, the match's
/// rate, and the time each type keeps its dead, which a match of no combat lacks.
type DeathTimes<'w> = (
    Option<Res<'w, GroundHeights>>,
    Res<'w, Time>,
    Res<'w, MatchRate>,
    Option<Res<'w, ById<UnitType, OnDeath>>>,
);

/// The projectiles drawn of late, of their types and drawings.
type NewShots<'w, 's> = Query<
    'w,
    's,
    (&'static UnitType, &'static Drawing),
    (With<Projectile>, Added<Drawing>, Allow<Unpredicted>),
>;

/// A unit as its shot plays: its type, its attack and its drawing.
type Shooters<'w, 's> = Query<
    'w,
    's,
    (&'static UnitType, &'static ActionProgress, &'static Drawing),
    Allow<Unpredicted>,
>;

/// A draw group's material, its mesh and the mesh's entity, made as the group first holds a quad,
/// as Bevy's mesh allocator takes no mesh of no vertex, and whether the entity shows, by its place
/// among the groups.
#[derive(Debug)]
struct GroupMesh {
    material: Handle<FileMaterial>,
    drawn: Option<DrawnGroup>,
    shown: bool,
}

/// A draw group's mesh and its entity.
#[derive(Debug)]
struct DrawnGroup {
    mesh: Handle<Mesh>,
    entity: Entity,
}

/// Steps and draws the match's effects (effects.md, Particles run on the CPU and draw as
/// billboards of the one material).
#[derive(Debug)]
pub(crate) struct EffectsPlugin;

/// The draws a particle run makes from, the same in every match.
const SEED: u64 = 0x5EED_5EED_5EED_5EED;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        FileMaterialPlugin::require(app);
        app.init_resource::<MuzzleFlashes>();
        app.init_resource::<SeenShots>();
        app.init_resource::<SeenDeaths>();
        app.add_observer(SeenDeaths::see);
        app.add_systems(Startup, MatchEffects::load);
        app.add_systems(FixedPostUpdate, FireSeen::see_predicted);
        // A shot shows its flash over its state's nodes, as Zero Hour sets the state of the shot
        // before its weapon fires (`AIAttackFireWeaponState::update`).
        app.add_systems(
            Update,
            (
                FireSeen::see_received,
                MatchEffects::play_fires,
                MuzzleFlashes::hide_ended,
                MatchEffects::play_deaths,
                MatchEffects::fly,
                MatchEffects::follow,
                MatchEffects::step,
                MatchEffects::draw,
            )
                .chain()
                .after(DrawSystems)
                .after(DressSystems)
                .in_set(ViewSystems),
        );
    }
}

impl Follower {
    /// A follower of `root` for each system the last play of `run` attached to its object.
    fn last(run: &ParticleRun, root: Entity) -> impl Iterator<Item = Self> + '_ {
        let attached = run.attached().iter();
        attached.map(move |&system| Self { system, root })
    }
}

impl MatchEffects {
    /// Reads the mode's effects, and gives each draw group its material, of its texture; none for
    /// a mode of no effects.
    fn load(
        (load, origins, names): (
            Res<'_, ClientLoad>,
            Res<'_, TypeOriginBook>,
            Res<'_, MatchNames>,
        ),
        assets: Res<'_, AssetServer>,
        mut materials: ResMut<'_, Assets<FileMaterial>>,
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
                GroupMesh {
                    material: materials.add(FileBlend::particles(texture, group.blend)),
                    drawn: None,
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

    /// Plays each shot the client saw at its unit's barrel, the next of those its type's state
    /// names for the shot's weapon slot: its fire effect at the barrel's bone, or at its unit's
    /// drawing for a barrel of none or a slot of no barrel, as the game's weapon plays one its
    /// draw does not (`Weapon.cpp`); the barrel's muzzle flash; and its hit of a weapon of no
    /// projectile at its target's drawing (effects.md, A shot's effect at its barrel). Each shot
    /// it triggers as `ShotSeen` besides; a shot of a unit gone plays nothing, and a match of no
    /// effects shows the flashes alone.
    fn play_fires(
        mut effects: Option<ResMut<'_, Self>>,
        (looks, index, heights, time): (
            Res<'_, UnitLooks>,
            Res<'_, EntityIndex>,
            Option<Res<'_, GroundHeights>>,
            Res<'_, Time>,
        ),
        (mut shots, shooters): (ResMut<'_, SeenShots>, Shooters<'_, '_>),
        drawn: Query<'_, '_, &Drawing, Allow<Unpredicted>>,
        (mut seen, states): (Query<'_, '_, &mut FireSeen>, Query<'_, '_, &LookStates>),
        (places, children, names, mut visibilities): (
            Query<'_, '_, &GlobalTransform>,
            Query<'_, '_, &'static Children>,
            Query<'_, '_, &'static Name>,
            Query<'_, '_, &'static mut Visibility>,
        ),
        (mut flashes, mut commands): (ResMut<'_, MuzzleFlashes>, Commands<'_, '_>),
    ) {
        let ground = Self::ground(heights.as_deref());
        let mut parts = FlashParts {
            children: &children,
            names: &names,
            visibilities: &mut visibilities,
            now: time.elapsed(),
        };
        for shot in shots.0.drain(..) {
            let Ok((&unit_type, progress, drawing)) = shooters.get(shot.unit) else {
                continue;
            };
            let root = drawing.root();
            let Ok(mut fired) = seen.get_mut(root) else {
                continue;
            };
            commands.trigger(shot);
            let action = shot.action;
            let group = looks
                .weapons(unit_type)
                .iter()
                .position(|group| group.contains(&action));
            let barrel = group.and_then(|group| {
                let slot = u8::try_from(group).ok()?;
                let barrels = states.get(root).ok()?.barrels(&looks, slot)?;
                Some(&barrels[fired.turns.turn(group, barrels.len())])
            });
            if let Some(flash) = barrel.and_then(|barrel| barrel.flash.as_ref()) {
                flashes.show(&mut parts, root, flash);
            }
            let Some(effects) = effects.as_deref_mut() else {
                continue;
            };
            if let Some(&fire) = effects.plays.fires.get(&action) {
                let bone = barrel.and_then(|barrel| barrel.bone.as_deref());
                let at = bone
                    .and_then(|bone| Self::bone(root, bone, &children, &names))
                    .unwrap_or(root);
                if let Ok(place) = places.get(at) {
                    effects
                        .run
                        .play(&effects.book, fire, place.affine(), &ground);
                    effects.follow_last(root);
                }
            }
            if let Some(&hit) = effects.plays.hits.get(&action) {
                let target = progress.attacking().and_then(|target| index.get(target));
                let target = target.and_then(|target| drawn.get(target).ok());
                if let Some(place) = target.and_then(|drawing| places.get(drawing.root()).ok()) {
                    effects
                        .run
                        .play(&effects.book, hit, place.affine(), &ground);
                }
            }
        }
    }

    /// Plays each unit type's death in its phases (effects.md, A death plays its phases): its
    /// start where its unit's drawing stood as the unit died, the unit despawned since or not;
    /// its middle at the middle of the time its type keeps it dead, and its end as it goes,
    /// each where its drawing stands then, or stood last. A rollback across a predicted death,
    /// which takes `Dead` away and gives it back in one frame from a unit that stays, plays none
    /// again, and a unit that lives again plays no more of its death.
    fn play_deaths(
        effects: Option<ResMut<'_, Self>>,
        (heights, time, rate, on_death): DeathTimes<'_>,
        (mut deaths, mut revived): (ResMut<'_, SeenDeaths>, RemovedComponents<'_, '_, Dead>),
        (units, places): (
            Query<'_, '_, Has<Dead>, Allow<Unpredicted>>,
            Query<'_, '_, &GlobalTransform>,
        ),
        (mut back, mut under_way): (Local<'_, Vec<Entity>>, Local<'_, Vec<DeathUnderWay>>),
    ) {
        back.clear();
        back.extend(revived.read());
        back.sort_unstable();
        let Some(mut effects) = effects else {
            deaths.0.clear();
            return;
        };
        let Self {
            book,
            run,
            plays,
            followers,
            ..
        } = &mut *effects;
        let ground = Self::ground(heights.as_deref());
        let now = time.elapsed();
        for death in deaths.played(&back, |unit| units.contains(unit)) {
            let Some(plays) = plays.deaths.get(&death.unit_type) else {
                continue;
            };
            for &list in &plays.start {
                run.play(book, list, death.place, &ground);
                followers.extend(Follower::last(run, death.root));
            }
            if plays.middle.is_empty() && plays.end.is_empty() {
                continue;
            }
            let kept = on_death
                .as_ref()
                .and_then(|book| book.get(death.unit_type))
                .and_then(|&on_death| on_death.kept(*rate));
            let middle = kept.map(|ticks| {
                let ticks = u32::try_from(ticks.get()).expect("a keep's ticks fit a u32");
                now + rate.length() * ticks / 2
            });
            under_way.push(DeathUnderWay {
                death,
                middle: middle.filter(|_| !plays.middle.is_empty()),
            });
        }
        under_way.retain_mut(|way| {
            let Some(plays) = plays.deaths.get(&way.death.unit_type) else {
                return false;
            };
            let unit = match units.get(way.death.unit) {
                Ok(true) => UnitNow::Dead,
                Ok(false) => UnitNow::Living,
                Err(_) => UnitNow::Gone,
            };
            let place = places.get(way.death.root).ok().map(GlobalTransform::affine);
            let (lists, stays) = match way.step(unit, place, now) {
                DeathStep::Waits => return true,
                DeathStep::Ends => return false,
                DeathStep::Middle => (&plays.middle, true),
                DeathStep::End => (&plays.end, false),
            };
            for &list in lists {
                run.play(book, list, way.death.place, &ground);
            }
            stays
        });
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
        self.followers.extend(Follower::last(&self.run, root));
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
            let Some(drawn) = &group.drawn else {
                if shown {
                    let mesh = meshes.add(Self::mesh_of(quads));
                    let entity = commands
                        .spawn((
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(group.material.clone()),
                            Transform::IDENTITY,
                            NoFrustumCulling,
                            NotShadowCaster,
                            NotShadowReceiver,
                            Pickable::IGNORE,
                        ))
                        .id();
                    group.drawn = Some(DrawnGroup { mesh, entity });
                    group.shown = true;
                }
                continue;
            };
            if shown != group.shown {
                group.shown = shown;
                let visibility = if shown {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                commands.entity(drawn.entity).insert(visibility);
            }
            if !shown {
                continue;
            }
            let Some(mut mesh) = meshes.get_mut(&drawn.mesh) else {
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

    /// The mesh of `quads`, a group's first.
    fn mesh_of(quads: &GroupQuads) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, quads.positions.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, quads.uvs.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, quads.colors.clone())
        .with_inserted_indices(Indices::U32(quads.indices.clone()))
    }
}
