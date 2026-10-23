use campfire_codec::Toml;
use campfire_math::Num;

use super::*;
use crate::client_model::{ModelScale, ModelTop};
use crate::client_unit::{DrawnLook, LookVariation};

#[test]
fn units_write_and_read_back_their_models_and_hidden_nodes() {
    let text = r#"
        [units.humvee]
        models = [
            { model = "client/models/avhummer.glb", hide = ["turretup01", "muzzlefx01"], top = 2.5 },
            { model = "client/models/avhumflag.glb", top = 3.25 },
            { model = "client/models/avhumfx.glb" },
        ]

        [units.tree]
        models = [{ model = "client/models/ptxbirch04.glb", scale = 1.25 }]

        [units.barracks]
        models = []
        placement_angle = "-135"

        [objects.tree]
        models = [{ model = "client/models/ptxbirch04.glb", scale = 1.25 }]
    "#;
    let units: ClientUnits = Toml::parse(text).unwrap();
    let humvee = &units.units["humvee"].models[0];
    assert_eq!(humvee.model.as_str(), "client/models/avhummer.glb");
    assert_eq!(humvee.hide, ["turretup01", "muzzlefx01"]);
    assert!(units.units["tree"].models[0].hide.is_empty());
    // The look's top, its flag's 3.25 m over the hull's 2.5 m, of a model whose top it does
    // not know, which it does not write; none of a look of no known top.
    let top = |look: &str| units.units[look].own().top().map(ModelTop::get);
    assert_eq!((top("humvee"), top("tree")), (Some(3.25), None));
    assert_eq!(Toml::write(&units).matches("top = ").count(), 2);
    // A model's scale, 1 when it gives none, which it does not write; a client object's look.
    assert_eq!(
        (humvee.scale, units.units["tree"].models[0].scale),
        (ModelScale::ONE, ModelScale::new(1.25).unwrap())
    );
    assert!(!Toml::write(&units).contains("scale = 1.0"));
    assert_eq!(units.objects["tree"].models, units.units["tree"].models);
    // A build's angle; none for a look that gives none, which it does not write.
    assert_eq!(
        units.units["barracks"].placement_angle,
        Some(Num::int(-135))
    );
    assert_eq!(units.units["tree"].placement_angle, None);
    assert_eq!(Toml::write(&units).matches("placement_angle").count(), 1);
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
    for scale in ["0.0", "-0.0", "-1.0", "nan", "inf"] {
        let text = format!("[units.a]\nmodels = [{{ model = \"a.glb\", scale = {scale} }}]");
        assert_eq!(
            refusal(&text),
            "a model's scale is positive and finite",
            "{scale}"
        );
    }
    // A top not finite; a top below the origin reads.
    let top = |top: &str| format!("[units.a]\nmodels = [{{ model = \"a.glb\", top = {top} }}]");
    for wrong in ["nan", "-inf"] {
        assert_eq!(refusal(&top(wrong)), "a model's top is finite", "{wrong}");
    }
    Toml::parse::<ClientUnits>(&top("-0.5")).unwrap();
    assert_eq!(
        refusal("[units.a]\nmodels = [{ model = \"../x.glb\" }]"),
        "\"../x.glb\": no path in the package"
    );
    let name = refusal("[units.A]\nmodels = []");
    assert!(name.starts_with("\"A\" is not a name"), "{name}");
    let field = refusal("[units.a]\nmodels = []\nmodel = \"x\"");
    assert!(field.starts_with("unknown field `model`"), "{field}");
    // A client object's look of a field of a unit's alone fails to read.
    for field in ["turret", "placement_angle", "variations"] {
        let text = format!("[units]\n[objects.a]\nmodels = []\n{field} = []");
        let refused = refusal(&text);
        assert!(
            refused.starts_with(&format!("unknown field `{field}`")),
            "{refused}"
        );
    }
}

#[test]
fn a_look_draws_its_variant_that_best_matches_the_maps_conditions() {
    let text = r#"
        [units.barracks]
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
        let mut look = units.units["barracks"].clone();
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

    // A look of two variations, the first of a night variant and a turret of its own: on a
    // night map the first draws its night models and the second its own, and the look's own
    // models take their night variant as well.
    let text = r#"
        [units.technical]
        models = [{ model = "parent.glb" }]
        variants = [{ conditions = ["night"], models = [{ model = "parent_night.glb" }] }]
        variations = [
            {
                models = [{ model = "one.glb", top = 2.0 }],
                variants = [{ conditions = ["night"], models = [{ model = "one_night.glb" }] }],
                turret = "gun",
            },
            { models = [{ model = "two.glb", top = 3.0 }] },
        ]
    "#;
    let units: ClientUnits = Toml::parse(text).unwrap();
    assert_eq!(
        Toml::parse::<ClientUnits>(&Toml::write(&units)).unwrap(),
        units
    );
    let mut look = units.units["technical"].clone();
    look.settle(&[DeclaredName::new("night").unwrap()]);
    let path = |drawn: DrawnLook<'_>| drawn.models[0].model.as_str().to_owned();
    assert_eq!(path(look.own()), "parent_night.glb");
    let drawn: Vec<DrawnLook<'_>> = look.variations.iter().map(LookVariation::drawn).collect();
    let paths: Vec<String> = drawn.iter().map(|&drawn| path(drawn)).collect();
    assert_eq!(paths, ["one_night.glb", "two.glb"]);
    assert!(
        look.variations
            .iter()
            .all(|variation| variation.variants.is_empty())
    );
    // Each variation's turret and top are its own: the first's node, and the second's 3 m;
    // the first's night models give no top.
    let turrets: Vec<Option<&str>> = drawn.iter().map(|drawn| drawn.turret).collect();
    assert_eq!(turrets, [Some("gun"), None]);
    let tops: Vec<Option<f32>> = drawn
        .iter()
        .map(|drawn| drawn.top().map(ModelTop::get))
        .collect();
    assert_eq!(tops, [None, Some(3.0)]);
}
