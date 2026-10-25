use campfire_codec::Toml;
use campfire_math::Num;

use super::*;
use crate::client_model::{ModelScale, ModelTop};
use crate::condition_test::LifeShare;

#[test]
fn units_write_and_read_back_their_states_conditions_and_models() {
    let text = r#"
        [units.humvee]
        draws = [{ states = [
            { sets = [[]], models = [
                { model = "client/models/avhummer.glb", hide = ["turretup01", "muzzlefx01"], top = 2.5 },
                { model = "client/models/avhumflag.glb", top = 3.25 },
                { model = "client/models/avhumfx.glb" },
            ] },
            { sets = [["damaged"], ["damaged", "night"]], models = [{ model = "client/models/avhummer_d.glb" }] },
        ] }]

        [units.lamp]
        draws = [{ states = [{ sets = [[], ["night"]], models = [] }], ignore = ["night"] }]

        [units.tree]
        draws = [{ states = [{ sets = [[]], models = [{ model = "client/models/ptxbirch04.glb", scale = 1.25 }] }] }]

        [units.barracks]
        draws = []
        placement_angle = "-135"

        [conditions]
        night = { test = "map" }
        damaged = { test = "life", above = "0.25", at_most = "0.5" }
        moving = { test = "moving", still_ms = 250 }
        upgraded = { test = "tag", tag = "weapon_upgrade" }
        rubble = { test = "never" }

        [objects.tree]
        models = [{ model = "client/models/ptxbirch04.glb", scale = 1.25 }]
    "#;
    let units: ClientUnits = Toml::parse(text).unwrap();
    let states = units.units["humvee"].draws[0].states();
    let humvee = &states[0].models[0];
    assert_eq!(humvee.model.as_str(), "client/models/avhummer.glb");
    assert_eq!(humvee.hide, ["turretup01", "muzzlefx01"]);
    // A state's sets, its own and its aliases'.
    let damaged = DeclaredName::new("damaged").unwrap();
    let night = DeclaredName::new("night").unwrap();
    assert_eq!(
        states[1].sets,
        [vec![damaged.clone()], vec![damaged, night]]
    );
    // A draw's ignored conditions.
    let ignore = units.units["lamp"].draws[0].ignore();
    assert_eq!(ignore, [DeclaredName::new("night").unwrap()]);
    // A state's top, its flag's 3.25 m over the hull's 2.5 m, of a model whose top it does not
    // know, which it does not write; none of a state of no known top.
    assert_eq!(states[0].top().map(ModelTop::get), Some(3.25));
    assert_eq!(states[1].top(), None);
    assert_eq!(Toml::write(&units).matches("top = ").count(), 2);
    // A model's scale, 1 when it gives none, which it does not write; a client object's look.
    let tree = &units.units["tree"].draws[0].states()[0].models;
    assert_eq!(
        (humvee.scale, tree[0].scale),
        (ModelScale::ONE, ModelScale::new(1.25).unwrap())
    );
    assert!(!Toml::write(&units).contains("scale = 1.0"));
    assert_eq!(&units.objects["tree"].models, tree);
    // A build's angle; none for a look that gives none, which it does not write.
    assert_eq!(
        units.units["barracks"].placement_angle,
        Some(Num::int(-135))
    );
    assert_eq!(units.units["tree"].placement_angle, None);
    assert_eq!(Toml::write(&units).matches("placement_angle").count(), 1);
    // Each condition's test.
    let test = |name: &str| units.conditions[name].clone();
    assert_eq!(test("night"), ConditionTest::Map);
    let quarter = Num::from_bits(Num::ONE.to_bits() / 4);
    let half = Num::from_bits(Num::ONE.to_bits() / 2);
    assert_eq!(
        test("damaged"),
        ConditionTest::Life(LifeShare::new(quarter, half).unwrap())
    );
    assert_eq!(test("moving"), ConditionTest::Moving { still_ms: 250 });
    let tag = DeclaredName::new("weapon_upgrade").unwrap();
    assert_eq!(test("upgraded"), ConditionTest::Tag { tag });
    assert_eq!(test("rubble"), ConditionTest::Never);
    assert_eq!(
        Toml::parse::<ClientUnits>(&Toml::write(&units)).unwrap(),
        units
    );

    // A scale of 0, below 0 or not finite; a path that leaves the package, a name that is no
    // declared name, an unknown field.
    let refusal = |text: &str| {
        let refused = Toml::parse::<ClientUnits>(text).unwrap_err();
        refused.message().to_owned()
    };
    let one = |model: &str| {
        format!("[units.a]\ndraws = [{{ states = [{{ sets = [[]], models = [{model}] }}] }}]")
    };
    for scale in ["0.0", "-0.0", "-1.0", "nan", "inf"] {
        let text = one(&format!("{{ model = \"a.glb\", scale = {scale} }}"));
        assert_eq!(
            refusal(&text),
            "a model's scale is positive and finite",
            "{scale}"
        );
    }
    // A top not finite; a top below the origin reads.
    let top = |top: &str| one(&format!("{{ model = \"a.glb\", top = {top} }}"));
    for wrong in ["nan", "-inf"] {
        assert_eq!(refusal(&top(wrong)), "a model's top is finite", "{wrong}");
    }
    Toml::parse::<ClientUnits>(&top("-0.5")).unwrap();
    assert_eq!(
        refusal(&one("{ model = \"../x.glb\" }")),
        "\"../x.glb\": no path in the package"
    );
    let name = refusal("[units.A]\ndraws = []");
    assert!(name.starts_with("\"A\" is not a name"), "{name}");
    let field = refusal("[units.a]\ndraws = []\nmodels = []");
    assert!(field.starts_with("unknown field `models`"), "{field}");
    // A client object's look of a field of a unit's alone fails to read.
    for field in ["turret", "placement_angle", "variations", "draws"] {
        let text = format!("[units]\n[objects.a]\nmodels = []\n{field} = []");
        let refused = refusal(&text);
        assert!(
            refused.starts_with(&format!("unknown field `{field}`")),
            "{refused}"
        );
    }
    // A draw of no state; a state of no set; a state of a condition of no test, in a variation
    // too; a life share whose bounds do not rise, or pass 1.
    let draws = |draws: &str| format!("[units.a]\ndraws = {draws}");
    let empty = "a draw has no state, or a state of no set";
    assert!(refusal(&draws("[{ states = [] }]")).starts_with(empty));
    assert!(refusal(&draws("[{ states = [{ sets = [], models = [] }] }]")).starts_with(empty));
    let untested = "a look's state names the condition `night`, of no test";
    let at_night = "[{ states = [{ sets = [[\"night\"]], models = [] }] }]";
    assert_eq!(refusal(&draws(at_night)), untested);
    let varied = format!("[units.a]\ndraws = []\nvariations = [{{ draws = {at_night} }}]");
    assert_eq!(refusal(&varied), untested);
    let ignored = "[{ states = [{ sets = [[]], models = [] }], ignore = [\"night\"] }]";
    assert_eq!(refusal(&draws(ignored)), untested);
    let share = |above: &str, at_most: &str| {
        format!(
            "[units]\n[conditions]\nhurt = {{ test = \"life\", above = \"{above}\", at_most = \"{at_most}\" }}"
        )
    };
    let bounds = "a life share's bounds do not rise within 0 and 1";
    for (above, at_most) in [
        ("0.5", "0.5"),
        ("0.5", "0.25"),
        ("0", "1.5"),
        ("-0.5", "0.5"),
    ] {
        assert!(
            refusal(&share(above, at_most)).starts_with(bounds),
            "{above} {at_most}"
        );
    }
}

#[test]
fn a_life_share_holds_a_life_above_its_lower_bound_and_at_most_its_upper() {
    // Above 0.25 and at most 0.5 of 8: above 2, at most 4.
    let quarter = Num::from_bits(Num::ONE.to_bits() / 4);
    let half = Num::from_bits(Num::ONE.to_bits() / 2);
    let share = LifeShare::new(quarter, half).unwrap();
    let holds = |current: i64| share.holds(Num::int(current), Num::int(8));
    assert_eq!(
        [1, 2, 3, 4, 5].map(holds),
        [false, false, true, true, false]
    );
    // The least step past 2 is above it, as the products are exact.
    assert!(share.holds(Num::int(2) + Num::from_bits(1), Num::int(8)));
    // A most of none holds no share; a share from 0 holds no life of none.
    assert!(!share.holds(Num::ZERO, Num::ZERO));
    let low = LifeShare::new(Num::ZERO, quarter).unwrap();
    assert_eq!(
        [0, 1, 2, 3].map(|current| low.holds(Num::int(current), Num::int(8))),
        [false, true, true, false]
    );
}

#[test]
fn an_object_draws_its_variant_that_best_matches_the_maps_conditions() {
    let text = r#"
        [units]
        [objects.barracks]
        models = [{ model = "day.glb" }]
        variants = [
            { conditions = ["night"], models = [{ model = "night.glb" }] },
            { conditions = ["snow"], models = [{ model = "snow.glb" }] },
            { conditions = ["night", "snow"], models = [{ model = "both.glb" }] },
            { conditions = ["night", "damaged"], models = [{ model = "hurt.glb" }] },
        ]
    "#;
    let units: ClientUnits = Toml::parse(text).unwrap();
    assert_eq!(
        Toml::parse::<ClientUnits>(&Toml::write(&units)).unwrap(),
        units
    );
    let drawn = |conditions: &[&str]| {
        let conditions: Vec<DeclaredName> = conditions
            .iter()
            .map(|condition| DeclaredName::new(condition).unwrap())
            .collect();
        let mut look = units.objects["barracks"].clone();
        look.settle(&conditions);
        assert!(look.variants.is_empty());
        look.models[0].model.as_str().to_owned()
    };
    // A day map has no condition in common with a variant: the look's own. A night map: the
    // night variant, 1 in common and none lacked, before both, of 1 and 1 lacked, and the
    // damaged night, of 1 and 1. Night and snow: both, of 2. Rain, of none: the look's own.
    assert_eq!(drawn(&[]), "day.glb");
    assert_eq!(drawn(&["night"]), "night.glb");
    assert_eq!(drawn(&["snow"]), "snow.glb");
    assert_eq!(drawn(&["night", "snow"]), "both.glb");
    assert_eq!(drawn(&["rain"]), "day.glb");
    // Night and damage: the damaged night, of 2 in common, past the night's 1.
    assert_eq!(drawn(&["night", "damaged"]), "hurt.glb");
}
