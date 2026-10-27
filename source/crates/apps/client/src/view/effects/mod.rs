use bevy::app::{App, FixedPostUpdate, Plugin, Startup, Update};
use bevy::asset::{AssetPath, AssetServer, Assets, Handle, RenderAssetUsages};
use bevy::camera::Camera3d;
use bevy::camera::visibility::{NoFrustumCulling, Visibility};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::lifecycle::RemovedComponents;
use bevy::ecs::name::Name;
use bevy::ecs::query::With;
use bevy::ecs::query::{Added, Allow, Has, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut, Single};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::math::{Affine3A, Quat, Vec3};
use bevy::mesh::{Indices, Mesh, Mesh3d, PrimitiveTopology, VertexAttributeValues};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::picking::Pickable;
use bevy::time::{Fixed, Time};
use bevy::transform::components::GlobalTransform;
use bevy::transform::components::Transform;
use bevy::world_serialization::WorldAssetRoot;
use campfire_actions::ActionProgress;
use campfire_client_files::{TerrainFile, WreckGround};
use campfire_combat::OnDeath;
use campfire_declarations::ById;
use campfire_host::ReceivedTick;
use campfire_projectiles::Projectile;
use campfire_script::MatchNames;
use campfire_sim::{EntityIndex, MatchRate, Unpredicted};
use campfire_units::{Dead, Owner, TypeOriginBook};
use campfire_values::{PackageIndex, Team, UnitType};

use crate::view::client_data::ClientLoad;
use crate::view::drawing::{Drawing, TearOff, Undrawn};
use crate::view::effects::chassis_run::{ChassisRun, Ride, Tilts};
use crate::view::effects::effect_book::{EffectBook, GroupId};
use crate::view::effects::effect_plays::{EffectPlays, PlaySources};
use crate::view::effects::fall_run::{FallEvent, FallFrom, FallRun, FallStart, MomentPlay};
use crate::view::effects::fire_seen::{FireSeen, SeenShots};
use crate::view::effects::muzzle_flashes::{FlashParts, MuzzleFlashes};
use crate::view::effects::particle_quads::{GroupQuads, ParticleQuads, ViewAxes};
use crate::view::effects::particle_run::{HeardAt, ParticleRun, SoundSource, SystemKey};
use crate::view::effects::seen_deaths::{DeathStep, DeathUnderWay, SeenDeaths, UnitNow};
use crate::view::effects::step_clock::StepClock;
use crate::view::effects::track_run::TrackRun;
use crate::view::effects::tread_run::{Driven, TreadRun};
use crate::view::effects::wreck_run::WreckRun;
use crate::view::file_material::{FileBlend, FileMaterial, FileMaterialPlugin};
use crate::view::glide::{FrameClock, Glide};
use crate::view::ground_heights::GroundHeights;
use crate::view::look_states::LookStates;
use crate::view::match_colors::Paint;
use crate::view::model_scenes::ModelScenes;
use crate::view::package_source::PackageAssets;
use crate::view::unit_looks::UnitLooks;
use crate::view::unit_models::DressSystems;
use crate::view::{DrawSystems, ViewSystems};

pub(crate) mod chassis_run;
pub(crate) mod effect_book;
pub(crate) mod effect_plays;
pub(crate) mod fall_run;
pub(crate) mod fire_seen;
pub(crate) mod muzzle_flashes;
pub(crate) mod particle_quads;
pub(crate) mod particle_run;
pub(crate) mod seen_deaths;
pub(crate) mod step_clock;
pub(crate) mod track_run;
pub(crate) mod tread_run;
pub(crate) mod wreck_run;

/// The match's effects, of the mode's own package's effects file: its book, the particle systems
/// alive, the clock their steps keep, the frame's quads, and each draw group's mesh and its
/// entity's visibility, hidden while it holds no quad.
#[derive(Resource, Debug)]
pub(crate) struct MatchEffects {
    pub(crate) book: EffectBook,
    pub(crate) run: ParticleRun,
    wrecks: WreckRun,
    falls: FallRun,
    treads: TreadRun,
    tracks: TrackRun,
    chassis: ChassisRun,
    /// How the wrecks meet the ground, as the effects file says.
    ground: WreckGround,
    plays: EffectPlays,
    clock: StepClock,
    quads: ParticleQuads,
    meshes: Vec<GroupMesh>,
    track_meshes: Vec<GroupMesh>,
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
/// rate, the time each type keeps its dead, which a match of no combat lacks, and the server's
/// state the client holds.
type DeathTimes<'w> = (
    Option<Res<'w, GroundHeights>>,
    Res<'w, Time>,
    Res<'w, MatchRate>,
    Option<Res<'w, ById<UnitType, OnDeath>>>,
    Res<'w, ReceivedTick>,
);

/// What a death's play keeps between its frames: the units that live again this frame, the
/// deaths under way, and the drawings this frame tears off their units.
type DeathScratch<'s> = (
    Local<'s, Vec<Entity>>,
    Local<'s, Vec<DeathUnderWay>>,
    Local<'s, Vec<TearOff>>,
);

/// What an aircraft's fall starts from: each drawing's glide, and the fixed clock it glides on.
type FallStarts<'w, 's> = (Query<'w, 's, &'static Glide>, Res<'w, Time<Fixed>>);

/// What a death's wrecks are spawned by: the commands, the models' scenes and the assets they
/// load through.
type WreckSpawns<'w, 's> = (
    Commands<'w, 's>,
    ResMut<'w, ModelScenes>,
    Res<'w, AssetServer>,
);

/// The units drawn of late, of their types and drawings.
type NewDrawings<'w, 's> = Query<
    'w,
    's,
    (&'static UnitType, &'static Drawing),
    (Without<Projectile>, Added<Drawing>, Allow<Unpredicted>),
>;

/// The projectiles drawn of late, of their types and drawings.
type NewShots<'w, 's> = Query<
    'w,
    's,
    (&'static UnitType, &'static Drawing),
    (With<Projectile>, Added<Drawing>, Allow<Unpredicted>),
>;

/// A unit as its shot plays: its type, its attack, its drawing, and its player, whose relation
/// says who hears its fire.
type Shooters<'w, 's> = Query<
    'w,
    's,
    (
        &'static UnitType,
        &'static ActionProgress,
        &'static Drawing,
        Option<&'static Owner>,
        &'static Team,
    ),
    Allow<Unpredicted>,
>;

/// A draw group's material, its mesh and the mesh's entity, made as the group first holds a quad,
/// as Bevy's mesh allocator takes no mesh of no vertex, whether the entity shows, and what it
/// draws, by its place among the groups.
#[derive(Debug)]
struct GroupMesh {
    material: Handle<FileMaterial>,
    drawn: Option<DrawnGroup>,
    shown: bool,
    kind: GroupKind,
}

/// What a group draws: particles, which no shadow falls on, or a ground's marks, which take the
/// shadows the ground takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupKind {
    Particles,
    Marks,
}

/// A draw group's mesh and its entity.
#[derive(Debug)]
struct DrawnGroup {
    mesh: Handle<Mesh>,
    entity: Entity,
}

/// Steps and draws the match's effects (02-engine-core.md, Particles run on the CPU and draw as
/// billboards of the one material).
#[derive(Debug)]
pub(crate) struct EffectsPlugin;

/// A track's lift over the ground's quads, past the overlays' 1, so it draws over them where it
/// shares a depth with them.
const TRACK_BIAS: i32 = 2;

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
                MatchEffects::drive,
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
                    kind: GroupKind::Particles,
                }
            })
            .collect();
        // A track draws as the ground it lies on, of its own texture.
        let surface = load.terrain.as_ref().map(TerrainFile::surface);
        let track_meshes = plays
            .track_textures
            .iter()
            .map(|path| {
                let path = PackageAssets::asset_path(PackageIndex::MODE, path);
                let base = StandardMaterial {
                    base_color_texture: Some(assets.load(AssetPath::from(path))),
                    ..StandardMaterial::default()
                };
                let base = match surface {
                    Some(surface) => StandardMaterial {
                        metallic: surface.metallic,
                        perceptual_roughness: surface.roughness,
                        reflectance: surface.specular * 0.5,
                        ..base
                    },
                    None => base,
                };
                GroupMesh {
                    material: materials.add(FileBlend::marks(base, TRACK_BIAS)),
                    drawn: None,
                    shown: false,
                    kind: GroupKind::Marks,
                }
            })
            .collect();
        commands.insert_resource(Self {
            book,
            run: ParticleRun::new(SEED),
            wrecks: WreckRun::new(SEED),
            falls: FallRun::default(),
            treads: TreadRun::new(file.treads()),
            tracks: TrackRun::new(file.tracks(), plays.track_textures.len()),
            chassis: ChassisRun::default(),
            ground: file.wrecks(),
            plays,
            clock: StepClock::default(),
            quads: ParticleQuads::default(),
            meshes,
            track_meshes,
            flights: Vec::new(),
            followers: Vec::new(),
        });
    }

    /// Plays each shot the client saw at its unit's barrel, the next of those its type's state
    /// names for the shot's weapon slot: its fire effect at the barrel's bone, or at its unit's
    /// drawing for a barrel of none or a slot of no barrel, as the game's weapon plays one its draw
    /// does not (`Weapon.cpp`); the barrel's muzzle flash; and its hit of a weapon of no projectile
    /// at its target's drawing (02-engine-core.md, A shot's effect at its barrel). Each shot it
    /// triggers as `ShotSeen` besides; a shot of a unit gone plays nothing, and a match of no
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
            let Ok((&unit_type, progress, drawing, owner, &team)) = shooters.get(shot.unit) else {
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
            if let Some(&sound) = effects.plays.fire_sounds.get(&action)
                && let Ok(place) = places.get(root)
            {
                effects.run.hear(HeardAt {
                    sound,
                    at: place.translation(),
                    source: Some(SoundSource {
                        owner: owner.map(|owner| owner.player_slot()),
                        team,
                    }),
                });
            }
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

    /// Tears each aircraft's drawing off its unit as its death starts and falls it, playing its
    /// fall's start, or plays a jet's ground death on the ground (02-engine-core.md, An aircraft's
    /// death falls as a wreck), and gives a unit that lives again its drawing back. Plays each unit
    /// type's death in its phases (02-engine-core.md, A death plays its phases): its start where
    /// its unit's drawing stood as the unit died, the unit despawned since or not; its middle at
    /// the middle of the time its type keeps it dead, and its end as it decays, each where its
    /// drawing stands then, or stood last; a unit that leaves the client's sight first plays no
    /// more of it (04-capabilities/combat.md, A death's decay reaches its clients). A rollback
    /// across a predicted death, which takes `Dead` away and gives it back in one frame from a unit
    /// that stays, plays none again, and a unit that lives again plays no more of its death.
    fn play_deaths(
        effects: Option<ResMut<'_, Self>>,
        (heights, time, rate, on_death, received): DeathTimes<'_>,
        (mut deaths, mut revived): (ResMut<'_, SeenDeaths>, RemovedComponents<'_, '_, Dead>),
        (units, places): (
            Query<'_, '_, Has<Dead>, Allow<Unpredicted>>,
            Query<'_, '_, &GlobalTransform>,
        ),
        (mut back, mut under_way, mut torn): DeathScratch<'_>,
        (mut commands, mut scenes, assets): WreckSpawns<'_, '_>,
        (glides, fixed): FallStarts<'_, '_>,
    ) {
        back.clear();
        back.extend(revived.read());
        back.sort_unstable();
        for &unit in &*back {
            if units.get(unit) == Ok(false) {
                commands.entity(unit).remove::<Undrawn>();
            }
        }
        let Some(mut effects) = effects else {
            deaths.0.clear();
            return;
        };
        let Self {
            book,
            run,
            wrecks,
            falls,
            plays,
            followers,
            ..
        } = &mut *effects;
        let ground = Self::ground(heights.as_deref());
        let now = time.elapsed();
        let clock = FrameClock {
            timestep: fixed.timestep(),
            fraction: fixed.overstep_fraction(),
        };
        let mut spawn = |scene: &AssetPath<'static>, place: Transform| {
            let scene = WorldAssetRoot(scenes.scene(scene, &assets));
            commands.spawn((scene, place, Paint::default())).id()
        };
        for death in deaths.played(&back, |unit| units.contains(unit)) {
            if let Some(fall) = plays.falls.get(&death.unit_type)
                && let Ok(glide) = glides.get(death.root)
            {
                let from = FallFrom {
                    root: death.root,
                    unit_type: death.unit_type,
                    at: death.place,
                    velocity: glide.velocity(now, clock),
                };
                let below = ground(death.place.translation.x, death.place.translation.z);
                let moment = match falls.start(from, fall, below) {
                    FallStart::Falls(moment) => {
                        torn.push(TearOff {
                            unit: death.unit,
                            root: death.root,
                        });
                        moment
                    }
                    FallStart::OnGround(moment) => moment,
                };
                Self::play_moment(run, wrecks, book, moment, death.place, &ground, &mut spawn);
            }
            let Some(plays) = plays.deaths.get(&death.unit_type) else {
                continue;
            };
            for &list in &plays.start {
                run.play(book, list, death.place, &ground);
                followers.extend(Follower::last(run, death.root));
            }
            wrecks.throw_one(&plays.wrecks[0], death.place, &ground, &mut spawn);
            let [_, middle_wrecks, end_wrecks] = &plays.wrecks;
            let later = !(plays.middle.is_empty() && middle_wrecks.is_empty());
            if !later && plays.end.is_empty() && end_wrecks.is_empty() {
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
                middle: middle.filter(|_| later),
            });
        }
        under_way.retain_mut(|way| {
            let Some(plays) = plays.deaths.get(&way.death.unit_type) else {
                return false;
            };
            let unit = match units.get(way.death.unit) {
                Ok(true) => UnitNow::Dead,
                Ok(false) => UnitNow::Living,
                Err(_) => way.went(received.get()),
            };
            let place = places.get(way.death.root).ok().map(GlobalTransform::affine);
            let (lists, throws, stays) = match way.step(unit, place, now) {
                DeathStep::Waits => return true,
                DeathStep::Ends => return false,
                DeathStep::Middle => (&plays.middle, &plays.wrecks[1], true),
                DeathStep::End => (&plays.end, &plays.wrecks[2], false),
            };
            for &list in lists {
                run.play(book, list, way.death.place, &ground);
            }
            wrecks.throw_one(throws, way.death.place, &ground, &mut spawn);
            stays
        });
        for tear in torn.drain(..) {
            commands.queue(tear);
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

    /// Gives each new drawing of a type whose treads play their systems and their track, and of a
    /// type of a chassis its chassis, then runs, scales or stops each drawing's tread systems by
    /// its speed and lays its track as it moves, each of a drawing gone going (02-engine-core.md, A
    /// tread throws dust while it drives; 02-engine-core.md, A tread lays track marks;
    /// 02-engine-core.md, A chassis follows the ground on springs). A drawing's root stands in the
    /// world, so its transform is its place already in the frame it is spawned.
    fn drive(
        effects: Option<ResMut<'_, Self>>,
        (time, fixed, heights): (
            Res<'_, Time>,
            Res<'_, Time<Fixed>>,
            Option<Res<'_, GroundHeights>>,
        ),
        drawn: NewDrawings<'_, '_>,
        roots: Query<'_, '_, (&Transform, &Glide)>,
        mut commands: Commands<'_, '_>,
    ) {
        let Some(mut effects) = effects else {
            return;
        };
        let Self {
            book,
            run,
            treads,
            tracks,
            chassis,
            plays,
            ..
        } = &mut *effects;
        let now = time.elapsed();
        let clock = FrameClock {
            timestep: fixed.timestep(),
            fraction: fixed.overstep_fraction(),
        };
        for (unit_type, drawing) in &drawn {
            let root = drawing.root();
            let Ok((place, glide)) = roots.get(root) else {
                continue;
            };
            if let Some(&play) = plays.chassis.get(unit_type) {
                chassis.add(root, play, glide.velocity(now, clock));
                commands.entity(root).insert(Tilts);
            }
            let Some(tread) = plays.treads.get(unit_type) else {
                continue;
            };
            if !tread.systems.is_empty() {
                let at = place.compute_affine();
                treads.add(
                    root,
                    tread.systems.iter().map(|&id| run.start(book, id, at)),
                );
            }
            if let Some(track) = tread.track {
                tracks.bind(root, track.group, track.width);
            }
        }
        treads.drive(run, |root| {
            let (place, glide) = roots.get(root).ok()?;
            Some(Driven {
                at: place.compute_affine(),
                velocity: glide.velocity(now, clock),
            })
        });
        let ground = Self::ground(heights.as_deref());
        tracks.lay(now, &ground, |root| {
            roots.get(root).ok().map(|(place, _)| place.translation)
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
    /// heights, or level ground at 0 on a map of none: the chassis, the particles, the wrecks, and
    /// the falls, each hit and blow-up played where it happens, a blown up drawing gone; then turns
    /// each drawing of a chassis by its yaw and its tilt between its last two steps.
    fn step(
        (time, fixed): (Res<'_, Time>, Res<'_, Time<Fixed>>),
        effects: Option<ResMut<'_, Self>>,
        heights: Option<Res<'_, GroundHeights>>,
        (mut places, glides): (Query<'_, '_, &mut Transform>, Query<'_, '_, &Glide>),
        (mut commands, mut scenes, assets): WreckSpawns<'_, '_>,
        (mut gone, mut events): (Local<'_, Vec<Entity>>, Local<'_, Vec<FallEvent>>),
    ) {
        let Some(mut effects) = effects else {
            return;
        };
        let Self {
            book,
            run,
            wrecks,
            falls,
            chassis,
            ground: physics,
            plays,
            clock,
            ..
        } = &mut *effects;
        let step = book.step();
        let steps = clock.take(time.delta_secs_f64(), step);
        let ground = Self::ground(heights.as_deref());
        let now = time.elapsed();
        let frame = FrameClock {
            timestep: fixed.timestep(),
            fraction: fixed.overstep_fraction(),
        };
        let ride = |root| {
            let glide = glides.get(root).ok()?;
            let spot = glide.at(now, frame);
            let normal = heights.as_deref().map_or(Vec3::Y, |heights| {
                heights.normal(spot.point.x, spot.point.z)
            });
            Some(Ride {
                yaw: spot.yaw,
                velocity: glide.velocity(now, frame),
                normal,
            })
        };
        let mut spawn = |scene: &AssetPath<'static>, place: Transform| {
            let scene = WorldAssetRoot(scenes.scene(scene, &assets));
            commands.spawn((scene, place, Paint::default())).id()
        };
        for _ in 0..steps {
            chassis.step(step, ride);
            run.step(book, &ground);
            wrecks.step(step, *physics, &ground, &mut gone);
            falls.step(step, physics.gravity, &plays.falls, &ground, &mut events);
            for event in events.drain(..) {
                let (moment, at) = match event {
                    FallEvent::Hit { unit_type, at, .. } => (&plays.falls[&unit_type].hit, at),
                    FallEvent::BlowUp {
                        root,
                        unit_type,
                        at,
                    } => {
                        gone.push(root);
                        (&plays.falls[&unit_type].blow_up, at)
                    }
                };
                Self::play_moment(run, wrecks, book, moment, at, &ground, &mut spawn);
            }
        }
        for entity in gone.drain(..) {
            commands.entity(entity).despawn();
        }
        for (root, turn) in chassis.turns(clock.fraction(step)) {
            let (Ok(glide), Ok(mut place)) = (glides.get(root), places.get_mut(root)) else {
                continue;
            };
            let rotation = Quat::from_rotation_y(glide.at(now, frame).yaw) * turn;
            if place.rotation != rotation {
                place.rotation = rotation;
            }
        }
        if steps > 0 {
            for piece in wrecks.pieces() {
                if let Ok(mut place) = places.get_mut(piece.entity) {
                    *place = piece.transform();
                }
            }
            for fall in falls.falls() {
                if let Ok(mut place) = places.get_mut(fall.root) {
                    *place = fall.transform();
                }
            }
        }
    }

    /// Plays `moment` at `at`: one of its lists, and one of its wrecks' lists, each drawn at
    /// random.
    fn play_moment(
        run: &mut ParticleRun,
        wrecks: &mut WreckRun,
        book: &EffectBook,
        moment: &MomentPlay,
        at: Affine3A,
        ground: &impl Fn(f32, f32) -> f32,
        spawn: impl FnMut(&AssetPath<'static>, Transform) -> Entity,
    ) {
        run.play_one(book, &moment.lists, at, ground);
        wrecks.throw_one(&moment.wrecks, at, ground, spawn);
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
            group.show(quads, &mut meshes, &mut commands);
        }
        effects.tracks.build();
        for (at, group) in effects.track_meshes.iter_mut().enumerate() {
            group.show(effects.tracks.group(at), &mut meshes, &mut commands);
        }
    }
}

impl GroupMesh {
    /// Writes its mesh of `quads` in place, made with its entity as it first holds a quad, and
    /// shows the entity while it holds any.
    fn show(
        &mut self,
        quads: &GroupQuads,
        meshes: &mut Assets<Mesh>,
        commands: &mut Commands<'_, '_>,
    ) {
        let shown = !quads.indices.is_empty();
        let Some(drawn) = &self.drawn else {
            if shown {
                let mesh = meshes.add(Self::mesh_of(quads));
                let mut entity = commands.spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(self.material.clone()),
                    Transform::IDENTITY,
                    NoFrustumCulling,
                    NotShadowCaster,
                    Pickable::IGNORE,
                ));
                if self.kind == GroupKind::Particles {
                    entity.insert(NotShadowReceiver);
                }
                let entity = entity.id();
                self.drawn = Some(DrawnGroup { mesh, entity });
                self.shown = true;
            }
            return;
        };
        if shown != self.shown {
            self.shown = shown;
            let visibility = if shown {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            commands.entity(drawn.entity).insert(visibility);
        }
        if !shown {
            return;
        }
        let Some(mut mesh) = meshes.get_mut(&drawn.mesh) else {
            return;
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
