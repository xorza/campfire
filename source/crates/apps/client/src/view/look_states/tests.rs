use bevy::asset::AssetApp;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::world::World;
use bevy::tasks::{IoTaskPool, TaskPool};
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use campfire_client_files::ClientUnits;
use campfire_codec::Toml;
use campfire_math::Num;
use campfire_script::MatchNames;
use campfire_sim::IdAllocator;
use campfire_units::{BodyUnit, Meter, TypeOrigin, TypeOriginBook};
use campfire_values::PackageIndex;

use super::*;
use crate::test_app;
use crate::view::ViewPlugin;
use crate::view::client_data::ClientLoad;
use crate::view::unit_looks::{LookNodes, LookTypes};

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

#[test]
fn a_units_draw_takes_the_state_of_its_life_keeping_a_model_of_the_same_scene() {
    IoTaskPool::get_or_init(TaskPool::new);
    let mut app = test_app::view();
    app.add_plugins((ViewPlugin, LookStatesPlugin));
    app.init_asset::<WorldAsset>();
    let data = ClientLoad::of_looks([Toml::parse::<ClientUnits>(TANK).unwrap()]);
    let origins: TypeOriginBook = [TypeOrigin {
        package: PackageIndex::MODE,
        name: "tank".into(),
    }]
    .into_iter()
    .collect();
    let tank_type = origins.iter().next().unwrap().0;
    let names = MatchNames::default();
    let types = LookTypes {
        origins: &origins,
        variations: None,
        names: &names,
    };
    let looks = UnitLooks::of(types, &data);
    app.insert_resource(looks);
    app.insert_resource(data);
    app.update();
    let mut ids = IdAllocator::default();
    let life = Life::new(Meter::new(Num::int(8)).unwrap());
    let unit = app
        .world_mut()
        .spawn((
            ids.allocate(),
            BodyUnit,
            Position::from((0, 0, 0)),
            Team::new(0),
            tank_type,
            life,
        ))
        .id();
    app.update();
    let root = app.world().get::<Drawing>(unit).unwrap().root();
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
        app.update();
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
    app.update();
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
