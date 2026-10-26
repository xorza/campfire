use campfire_client_files::EffectsFile;
use campfire_codec::Toml;
use campfire_common::DeclaredName;
use campfire_units::TypeOrigin;

use super::*;

#[test]
fn a_type_plays_the_death_end_and_trail_its_look_names_of_those_the_book_holds() {
    // The book holds the blast, the hit and the smoke; the tank names a death of no list,
    // so it plays none; its damaged state names its first slot's bone, its default none.
    let effects: EffectsFile = Toml::parse(
        r#"
        step = 0.033333335
        cap = 10
        [systems.smoke]
        blend = { mode = "alpha" }
        texture = "client/textures/smoke.ktx2"
        ground_aligned = false
        up_towards_emitter = false
        above_ground_only = false
        angle = [0.0, 0.0]
        angular_rate = [0.0, 0.0]
        angular_damping = [1.0, 1.0]
        velocity_damping = [1.0, 1.0]
        gravity = 0.0
        drift = [0.0, 0.0, 0.0]
        lifetime = [10.0, 10.0]
        system_lifetime = 1
        size = [1.0, 1.0]
        start_size_rate = [0.0, 0.0]
        size_rate = [0.0, 0.0]
        size_rate_damping = [1.0, 1.0]
        alpha = [{ alpha = [1.0, 1.0], at = 0 }]
        color = [{ color = [1.0, 1.0, 1.0], at = 0 }]
        color_scale = [0.0, 0.0]
        burst_delay = [0.0, 0.0]
        burst_count = [1.0, 1.0]
        initial_delay = [0.0, 0.0]
        velocity = { kind = "still" }
        volume = { kind = "point" }
        hollow = false
        [[lists.blast.systems]]
        system = "smoke"
        count = 1
        offset = [0.0, 0.0, 0.0]
        radius = [0.0, 0.0]
        height = [0.0, 0.0]
        rotation = [0.0, 0.0, 0.0, 1.0]
        orient_to_object = false
        attach_to_object = false
        at_ground = false
        callers_radius = false
        ricochet = false
        "#,
    )
    .unwrap();
    let looks: ClientUnits = Toml::parse(
        r#"
        [units.bunker]
        draws = []
        death = "blast"
        [units.tank]
        draws = [{ states = [
            { sets = [[]], models = [] },
            { sets = [["damaged"]], models = [], fire_bones = [{ slot = 1, bone = "MuzzleFX01" }] },
        ] }]
        death = "fizzle"
        [units.shell]
        draws = []
        end = "blast"
        trail = "smoke"
        [conditions]
        damaged = { test = "map" }
        "#,
    )
    .unwrap();
    let book = EffectBook::new(&effects);
    let origins: TypeOriginBook = ["bunker", "tank", "shell"]
        .map(|name| TypeOrigin {
            package: PackageIndex::MODE,
            name: name.into(),
        })
        .into_iter()
        .collect();
    let plays = EffectPlays::of(PlaySources {
        book: &book,
        looks: &looks,
        origins: &origins,
        names: &MatchNames::default(),
    });
    let [bunker, tank, shell] = [0, 1, 2].map(|at| origins.iter().nth(at).unwrap().0);
    let blast = book.list_id(&DeclaredName::new("blast").unwrap()).unwrap();
    let smoke = book.id(&DeclaredName::new("smoke").unwrap()).unwrap();
    assert_eq!(plays.deaths, BTreeMap::from([(bunker, blast)]));
    assert_eq!(plays.ends, BTreeMap::from([(shell, blast)]));
    assert_eq!(plays.trails, BTreeMap::from([(shell, smoke)]));
    let slot = FireSlot {
        unit_type: tank,
        slot: 1,
    };
    assert_eq!(
        plays.bones,
        BTreeMap::from([(slot, "MuzzleFX01".to_owned())])
    );
    assert!(plays.fires.is_empty() && plays.hits.is_empty());
}
