use bevy::animation::AnimationClip;
use bevy::animation::graph::AnimationNodeIndex;
use bevy::asset::{AssetPath, Handle};
use bevy::ecs::bundle::Bundle;
use bevy::ecs::change_detection::Mut;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::system::SystemState;
use bevy::ecs::world::World;
use bevy::time::TimeUpdateStrategy;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use campfire_client_files::ClientUnits;
use campfire_codec::Toml;
use campfire_production::SiteStart;
use campfire_script::MatchNames;
use campfire_sim::IdAllocator;
use campfire_units::{BodyUnit, Meter, TypeOrigin, TypeOriginBook};
use campfire_values::{PackageIndex, Rank};

use super::*;
use crate::test_app;
use crate::view::ViewPlugin;
use crate::view::client_data::ClientLoad;
use crate::view::look_clips::{HiddenPivot, LookClip};
use crate::view::unit_looks::{LookNodes, LookTypes, Showing};
use crate::view::unit_models::{ModelPivots, PivotMeshes, Redress};

/// A tank whose state changes by its life: whole, of its flag hidden; damaged, of the same model
/// and no node hidden; really damaged, a wreck; and scratched, of no model.
const TANK: &str = r#"
    [units.tank]
    draws = [{ states = [
        { sets = [[]], models = [{ model = "tank.glb", hide = ["flag"] }] },
        { sets = [["damaged"]], models = [{ model = "tank.glb" }] },
        { sets = [["really"]], models = [{ model = "wreck.glb" }] },
        { sets = [["scratched"]], models = [] },
    ] }]

    [conditions]
    damaged = { test = "life", above = "0.25", at_most = "0.5" }
    really = { test = "life", above = "0", at_most = "0.25" }
    scratched = { test = "life", above = "0.75", at_most = "0.875" }
"#;

/// The models under `root`: each one's scene's file, and whether it waits to replace another.
fn models(world: &World, root: Entity) -> Vec<(String, bool)> {
    let children = world.get::<Children>(root).into_iter().flatten();
    children
        .map(|&model| {
            let scene = &world.get::<WorldAssetRoot>(model).unwrap().0;
            let path = scene.path().unwrap().to_string();
            let file = path
                .trim_start_matches("package://0/")
                .trim_end_matches("#Scene0");
            (file.to_owned(), world.get::<Replaces>(model).is_some())
        })
        .collect()
}

/// A view app that draws the looks of `units`, a units file, of the mode's one type `name`, and
/// that type.
fn looks_app(units: &str, name: &str) -> (App, UnitType) {
    let mut app = test_app::view();
    app.add_plugins((ViewPlugin, LookStatesPlugin));
    app.init_asset::<WorldAsset>();
    app.init_asset::<AnimationClip>();
    app.init_asset::<AnimationGraph>();
    let data = ClientLoad::of_looks([Toml::parse::<ClientUnits>(units).unwrap()]);
    let origins: TypeOriginBook = [TypeOrigin {
        package: PackageIndex::MODE,
        name: name.into(),
    }]
    .into_iter()
    .collect();
    let unit_type = origins.iter().next().unwrap().0;
    let names = MatchNames::default();
    let types = LookTypes {
        origins: &origins,
        variations: None,
        names: &names,
    };
    app.insert_resource(UnitLooks::of(types, &data));
    app.insert_resource(data);
    step(&mut app, 0);
    (app, unit_type)
}

/// Gives the clip of each file of `clips` its frames, at 30 a second, as its file loaded would.
fn timings(app: &mut App, clips: &[(&str, u32)]) {
    for &(file, frames) in clips {
        let path = AssetPath::from(format!("package://0/{file}"));
        let clip = app
            .world_mut()
            .resource_scope(|world, mut clips: Mut<'_, LookClips>| {
                world.resource_scope(|world, mut graphs: Mut<'_, Assets<AnimationGraph>>| {
                    clips.clip(&path, world.resource::<AssetServer>(), &mut graphs)
                })
            });
        let timing = ClipTiming {
            frames,
            rate: 30,
            hidden: Vec::new(),
        };
        let mut timings = app.world_mut().resource_mut::<Assets<ClipTiming>>();
        timings.insert(clip.timing.id(), timing).unwrap();
    }
}

/// Runs a frame `ms` milliseconds long.
fn step(app: &mut App, ms: u64) {
    let frame = Duration::from_millis(ms);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame));
    app.update();
}

/// Spawns a unit of `parts`, and runs a frame, which draws it: the unit, and its drawing's root.
fn drawn(app: &mut App, parts: impl Bundle) -> [Entity; 2] {
    let id = IdAllocator::default().allocate();
    let at = Position::from((0, 0, 0));
    let unit = app
        .world_mut()
        .spawn((id, BodyUnit, at, Team::new(0), parts))
        .id();
    step(app, 0);
    [unit, app.world().get::<Drawing>(unit).unwrap().root()]
}

#[test]
fn a_units_draw_takes_the_state_of_its_life_keeping_a_model_of_the_same_scene() {
    let (mut app, tank_type) = looks_app(TANK, "tank");
    let life = Life::new(Meter::new(Num::int(8)).unwrap());
    let [unit, root] = drawn(&mut app, (tank_type, life));
    // Whole, its first state's model is shown at once, its flag hidden; its scene alone is
    // loaded.
    assert_eq!(models(app.world(), root), [("tank.glb".to_owned(), false)]);
    let loaded = |app: &App| app.world().resource::<ModelScenes>().count();
    assert_eq!(loaded(&app), 1);
    let first = app.world().get::<Children>(root).unwrap()[0];
    let hidden = |app: &App, model| {
        let nodes = app.world().get::<LookNodes>(model).unwrap();
        nodes.hide.iter().cloned().collect::<Vec<String>>()
    };
    assert_eq!(hidden(&app, first), ["flag"]);
    // At half its life, damaged: the same model, which hides its nodes again, hiding none.
    let hurt = |app: &mut App, by: i64| {
        let mut life = app.world_mut().get_mut::<Life>(unit).unwrap();
        life.take(Num::int(by));
        step(app, 0);
    };
    hurt(&mut app, 4);
    assert_eq!(models(app.world(), root), [("tank.glb".to_owned(), false)]);
    assert_eq!(app.world().get::<Children>(root).unwrap()[0], first);
    assert!(hidden(&app, first).is_empty());
    assert!(app.world().get::<Redress>(first).is_some());
    // At 1 of 8, really damaged: the wreck's scene loads, and the wreck spawns hidden to replace
    // the tank, which stays drawn until the wreck is dressed.
    hurt(&mut app, 3);
    assert_eq!(loaded(&app), 2);
    assert_eq!(
        models(app.world(), root),
        [
            ("tank.glb".to_owned(), false),
            ("wreck.glb".to_owned(), true)
        ]
    );
    let wreck = app.world().get::<Children>(root).unwrap()[1];
    assert_eq!(
        app.world().get::<Visibility>(wreck),
        Some(&Visibility::Hidden)
    );
    assert_eq!(app.world().get::<Replaces>(wreck), Some(&Replaces(first)));
    // Healed before the wreck shows: the wreck goes, and a new tank replaces the first, still
    // drawn.
    app.world_mut()
        .get_mut::<Life>(unit)
        .unwrap()
        .add(Num::int(7));
    step(&mut app, 0);
    assert_eq!(
        models(app.world(), root),
        [
            ("tank.glb".to_owned(), false),
            ("tank.glb".to_owned(), true)
        ]
    );
    assert!(app.world().get_entity(wreck).is_err());
    let again = app.world().get::<Children>(root).unwrap()[1];
    assert_eq!(app.world().get::<Replaces>(again), Some(&Replaces(first)));
    // Scratched to 7 of 8 before the new tank shows, of no model: the new tank goes, and the
    // first with it.
    hurt(&mut app, 1);
    assert!(models(app.world(), root).is_empty());
    assert!(app.world().get_entity(first).is_err() && app.world().get_entity(again).is_err());
}

/// A tank of keyed states: at rest, a loop; damaged, its clip once, which a turn between the two
/// precedes; really damaged, a wreck that lets the damaged state finish its clip first.
const KEYED: &str = r#"
    [units.tank]
    draws = [{ states = [
        { sets = [[]], models = [{ model = "tank.glb" }], key = "rest", animation = { clips = [{ animation = "rest.glb" }], mode = "loop" } },
        { sets = [["damaged"]], models = [{ model = "tank.glb" }], key = "hurt", animation = { clips = [{ animation = "hurt.glb" }], mode = "once" } },
        { sets = [["really"]], models = [{ model = "wreck.glb" }], waits_for = "hurt" },
    ], transitions = [
        { from = "rest", to = "hurt", models = [{ model = "turn.glb" }], animation = { clips = [{ animation = "turn.glb" }], mode = "once" } },
    ] }]

    [conditions]
    damaged = { test = "life", above = "0.25", at_most = "0.5" }
    really = { test = "life", above = "0", at_most = "0.25" }
"#;

#[test]
fn a_draw_shows_the_transition_between_two_keys_and_lets_a_state_finish_its_clip() {
    let (mut app, tank_type) = looks_app(KEYED, "tank");
    // The rest's clip of 11 frames, the turn's of 3, the damaged one's of 4.
    timings(
        &mut app,
        &[("rest.glb", 11), ("turn.glb", 3), ("hurt.glb", 4)],
    );
    let life = Life::new(Meter::new(Num::int(8)).unwrap());
    let [unit, root] = drawn(&mut app, (tank_type, life));
    let show = |app: &App| app.world().get::<LookStates>(root).unwrap().draws[0].clone();
    let at = |app: &App| (show(app).showing(), show(app).pending());
    assert_eq!(at(&app), (Some(Showing::State(0)), None));
    // Damaged: the turn from rest to hurt shows first, its model to replace the tank's, the
    // damaged state next; the turn's clip stands at its first frame.
    let hurt = |app: &mut App, by: i64| {
        let mut life = app.world_mut().get_mut::<Life>(unit).unwrap();
        life.take(Num::int(by));
    };
    hurt(&mut app, 4);
    step(&mut app, 0);
    assert_eq!(at(&app), (Some(Showing::Transition(0)), Some(1)));
    assert_eq!(show(&app).playing(), Some((0, Some(0.0))));
    assert_eq!(
        models(app.world(), root),
        [
            ("tank.glb".to_owned(), false),
            ("turn.glb".to_owned(), true)
        ]
    );
    // A tenth of a second plays the turn's 3 frames to its last, 2, which this frame draws; the
    // next frame finds it complete, and shows the damaged state, its clip at its first frame.
    step(&mut app, 100);
    assert_eq!(at(&app), (Some(Showing::Transition(0)), Some(1)));
    assert_eq!(show(&app).playing(), Some((0, Some(2.0))));
    step(&mut app, 0);
    assert_eq!(at(&app), (Some(Showing::State(1)), None));
    assert_eq!(show(&app).playing(), Some((0, Some(0.0))));
    // Really damaged as its clip plays its first frame: the wreck waits for the damaged clip,
    // 0.3 of a frame on after 10 ms.
    hurt(&mut app, 3);
    step(&mut app, 10);
    assert_eq!(at(&app), (Some(Showing::State(1)), Some(2)));
    // A tenth of a second plays it to its last frame, 3; the next frame, the wreck shows, of no
    // clip.
    step(&mut app, 100);
    assert_eq!(show(&app).playing(), Some((0, Some(3.0))));
    step(&mut app, 0);
    assert_eq!(at(&app), (Some(Showing::State(2)), None));
    assert_eq!(
        models(app.world(), root).last(),
        Some(&("wreck.glb".to_owned(), true))
    );
}

#[test]
fn a_state_of_idle_clips_draws_another_as_each_completes() {
    let idle = r#"
        [units.man]
        draws = [{ states = [{ sets = [[]], models = [{ model = "man.glb" }], animation = { clips = [
            { animation = "a.glb" }, { animation = "b.glb" },
        ], idle = true, mode = "once" } }] }]
    "#;
    let (mut app, man_type) = looks_app(idle, "man");
    // Each clip of 3 frames.
    timings(&mut app, &[("a.glb", 3), ("b.glb", 3)]);
    let [_, root] = drawn(&mut app, man_type);
    let playing = |app: &App| app.world().get::<LookStates>(root).unwrap().draws[0].playing();
    // One clip plays from its first frame; a tenth of a second plays it to its last, 2; the next
    // frame, the other starts at its first.
    let (first, frame) = playing(&app).unwrap();
    assert_eq!(frame, Some(0.0));
    step(&mut app, 100);
    assert_eq!(playing(&app), Some((first, Some(2.0))));
    step(&mut app, 0);
    assert_eq!(playing(&app), Some((1 - first, Some(0.0))));
    // Each completion draws the other, never the one it played.
    for _ in 0..4 {
        let (before, _) = playing(&app).unwrap();
        step(&mut app, 100);
        step(&mut app, 0);
        assert_eq!(playing(&app).unwrap().0, 1 - before);
    }
}

#[test]
fn a_clip_hides_its_pivots_own_meshes_in_its_frames_but_those_its_look_hides() {
    use Visibility::{Hidden, Inherited};
    let mut world = World::new();
    let [player, arm, flag] = [(); 3].map(|()| world.spawn(Inherited).id());
    // The arm's own meshes, its arm and its flag, which the look hides; the clip hides the arm at
    // frames 1 and 2, and a leg the model lacks.
    let dressed = Dressed {
        hidden: vec![flag],
        pivots: Some(ModelPivots {
            player,
            meshes: vec![PivotMeshes {
                pivot: "arm".to_owned(),
                meshes: vec![arm, flag],
            }],
        }),
    };
    world.entity_mut(flag).insert(Hidden);
    let hidden_at = |pivot: &str| HiddenPivot {
        pivot: pivot.to_owned(),
        runs: vec![[1, 3]],
    };
    let timing = ClipTiming {
        frames: 4,
        rate: 30,
        hidden: vec![hidden_at("leg"), hidden_at("arm")],
    };
    let clip = LookClip {
        graph: Handle::default(),
        node: AnimationNodeIndex::new(0),
        timing: Handle::default(),
    };
    let mut clip_hidden = ClipHidden::default();
    let mut scratch = Vec::new();
    let mut visibilities = |clip_hidden: &mut ClipHidden, frame| {
        let mut state = SystemState::<Query<'_, '_, &'static mut Visibility>>::new(&mut world);
        let mut query = state.get_mut(&mut world).unwrap();
        let mut hiding = ClipHiding {
            scratch: &mut scratch,
            visibilities: &mut query,
        };
        let pose = ClipPose {
            clip: &clip,
            timing: &timing,
            seconds: 0.0,
            frame,
        };
        hiding.hide(&dressed, clip_hidden, pose);
        [arm, flag].map(|node| *world.get::<Visibility>(node).unwrap())
    };
    // Frame 0 hides none; frames 1 and 2 the arm, the flag hidden by the look all along; frame 3
    // shows the arm again. The clip's arm, its second hidden pivot, is the model's first pivot.
    assert_eq!(visibilities(&mut clip_hidden, 0), [Inherited, Hidden]);
    assert_eq!(
        clip_hidden.pivots,
        [ClipPivot {
            hidden: 1,
            pivot: 0
        }]
    );
    assert_eq!(clip_hidden.timing, Some(clip.timing.id()));
    assert_eq!(visibilities(&mut clip_hidden, 1), [Hidden, Hidden]);
    assert_eq!(clip_hidden.meshes, [arm]);
    assert_eq!(visibilities(&mut clip_hidden, 2), [Hidden, Hidden]);
    assert_eq!(visibilities(&mut clip_hidden, 3), [Inherited, Hidden]);
    assert!(clip_hidden.meshes.is_empty());
}

#[test]
fn a_rising_states_models_sink_by_the_share_of_their_site_not_yet_built() {
    let rising = r#"
        [units.depot]
        draws = [{ states = [{ sets = [[]], models = [{ model = "depot.glb" }], rise = "4" }] }]
    "#;
    let (mut app, depot_type) = looks_app(rising, "depot");
    // A site of 4 ticks, a tick built.
    let site = Site::new(SiteStart {
        action: ActionId::new(0),
        rank: Rank::FIRST,
        time: Num::int(4),
        gain: Num::ZERO,
        paid: &[],
        holder: None,
    });
    let mut progress = SiteProgress::default();
    progress.grow(&site, Num::ONE);
    let [unit, root] = drawn(&mut app, (depot_type, site.clone(), progress));
    step(&mut app, 0);
    let sunk = |app: &App| {
        let model = app.world().get::<Children>(root).unwrap()[0];
        app.world().get::<Transform>(model).unwrap().translation.y
    };
    // A quarter built: 3 of its 4 m below its place; three quarters, 1 m.
    assert_eq!(sunk(&app), -3.0);
    progress.grow(&site, Num::int(2));
    app.world_mut().entity_mut(unit).insert(progress);
    step(&mut app, 0);
    assert_eq!(sunk(&app), -1.0);
    // Complete, a site no more: at its place.
    app.world_mut()
        .entity_mut(unit)
        .remove::<(Site, SiteProgress)>();
    step(&mut app, 0);
    assert_eq!(sunk(&app), 0.0);
}
