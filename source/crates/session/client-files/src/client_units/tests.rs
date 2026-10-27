use campfire_codec::Toml;
use campfire_math::Num;

use super::*;
use crate::client_model::{ModelScale, ModelTop};
use crate::client_unit::{BarrelFlash, DeathLook, FireBarrel, FireBone};
use crate::condition_test::{LifeShare, SiteStage};
use crate::death_fall::{DeathFall, FallStage, GroundDeath, Spiral};
use crate::state_animation::{AnimationMode, SpeedRange, StartFrame};
use crate::state_set::StateSet;
use crate::tread_look::{TrackLook, TreadLook};
use crate::wreck::{Wreck, WreckLaunch, WreckSink};

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
            { sets = [["damaged"], ["damaged", "night"], { names = ["damaged"], untested = 2 }], models = [{ model = "client/models/avhummer_d.glb" }], fire_bones = [{ slot = 0, barrels = [{ bone = "MuzzleFX01", flash = { node = "avhummer_d.muzzlefx01", ms = 100 } }, {}] }] },
        ] }]
        death = { start = ["fx_humvee_death"], end = ["fx_humvee_blast"] }

        [units.crusader]
        draws = []
        tread = { systems = ["tread_dust"], track = { texture = "client/tracks/extnktrack.ktx2", width = "17.5" } }

        [units.crusader.death.wrecks]
        middle = [[{ models = ["client/models/avleopard_d1.glb"], count = 1, offset = ["0", "0", "0"], launch = { fling = { across = "1.2", up = ["0.297", "0.9"] } }, spin = "0.3", sink = { life = "1.5", delay = "1.5", rate = "2", gone = "8" } }]]
        end = [[{ models = ["client/models/avleopard_d2.glb", "client/models/avleopard_d3.glb"], count = 2, offset = ["0.5", "0", "7.7"], launch = { force = { speed = ["60", "90"], pitch = ["1.48", "1.57"] } }, spin = "5.2", sink = { delay = "1", rate = "1", gone = "2" } }]]

        [units.comanche]
        draws = []
        death = { start = ["fx_heli_hit"], fall = { gravity = "0.4", roll = "0", spiral = { turn = "2", speed = "40", spin = "6" }, hit = { effects = ["fx_heli_crash"] }, blow_up_after = "0.5", blow_up = { effects = ["fx_heli_blast"] } } }

        [units.raptor]
        draws = []
        death = { fall = { gravity = "0.6", roll = "1.5", start = { effects = ["fx_jet_start"] }, blow_up_after = "1", ground = { height = "0.64", stage = { effects = ["fx_jet_ground"] } } } }

        [units.humvee_shell]
        draws = []
        end = "fx_shell_hit"
        trail = "smoke_trail"

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
        fallen = { test = "dead" }
        engaged = { test = "attacking" }
        awaiting = { test = "site", stage = "waiting" }
        partly = { test = "site", stage = "started" }

        [objects.tree]
        models = [{ model = "client/models/ptxbirch04.glb", scale = 1.25 }]

        [actions.humvee_gun]
        fire = "fx_gun_fire"
    "#;
    let units: ClientUnits = Toml::parse(text).unwrap();
    let states = units.units["humvee"].draws[0].states();
    let humvee = &states[0].models[0];
    assert_eq!(humvee.model.as_str(), "client/models/avhummer.glb");
    assert_eq!(humvee.hide, ["turretup01", "muzzlefx01"]);
    // The damaged state's fire bone and flash, of the first slot; the look's death; the gun's fire, of no
    // hit; the shell's end and trail; none of what a look does not name, which it does not write.
    let name = |text: &str| DeclaredName::new(text).unwrap();
    assert_eq!(
        states[1].fire_bones,
        [FireBone {
            slot: 0,
            barrels: vec![
                FireBarrel {
                    bone: Some("MuzzleFX01".to_owned()),
                    flash: Some(BarrelFlash {
                        node: "avhummer_d.muzzlefx01".to_owned(),
                        ms: 100
                    })
                },
                FireBarrel::default()
            ]
        }]
    );
    assert_eq!(states[0].fire_bones, []);
    let humvee_death = DeathLook {
        start: vec![name("fx_humvee_death")],
        end: vec![name("fx_humvee_blast")],
        ..DeathLook::NONE
    };
    assert_eq!(units.units["humvee"].death, humvee_death);
    // A tank's tread, its hulk at the death's middle and its turret's two pieces at its end.
    let num = |text: &str| text.parse::<Num>().unwrap();
    let path = |text: &str| PackagePath::parse(text).unwrap();
    let crusader = &units.units["crusader"];
    assert_eq!(
        crusader.tread,
        TreadLook {
            systems: vec![name("tread_dust")],
            track: Some(TrackLook {
                texture: path("client/tracks/extnktrack.ktx2"),
                width: num("17.5"),
            }),
        }
    );
    let wrecks = &crusader.death.wrecks;
    assert_eq!(
        (wrecks.start.len(), wrecks.middle.len(), wrecks.end.len()),
        (0, 1, 1)
    );
    assert_eq!(
        wrecks.middle[0].0,
        [Wreck {
            models: vec![path("client/models/avleopard_d1.glb")],
            count: 1,
            offset: [Num::ZERO; 3],
            launch: WreckLaunch::Fling {
                across: num("1.2"),
                up: [num("0.297"), num("0.9")],
            },
            spin: num("0.3"),
            sink: WreckSink {
                life: Some(num("1.5")),
                delay: num("1.5"),
                rate: num("2"),
                gone: num("8"),
            },
        }]
    );
    assert_eq!(
        wrecks.end[0].0[0].launch,
        WreckLaunch::Force {
            speed: [num("60"), num("90")],
            pitch: [num("1.48"), num("1.57")],
        }
    );
    assert_eq!(wrecks.end[0].0[0].sink.life, None);
    // A helicopter's fall, which spirals and blows up after it hits.
    assert_eq!(
        units.units["comanche"].death.fall,
        Some(DeathFall {
            gravity: num("0.4"),
            roll: Num::ZERO,
            spiral: Some(Spiral {
                turn: num("2"),
                speed: num("40"),
                spin: num("6"),
            }),
            start: FallStage::default(),
            hit: FallStage {
                effects: vec![name("fx_heli_crash")],
                wrecks: Vec::new(),
            },
            blow_up_after: num("0.5"),
            blow_up: FallStage {
                effects: vec![name("fx_heli_blast")],
                wrecks: Vec::new(),
            },
            ground: None,
        })
    );
    // A jet's fall, which rolls, and its death on the ground, played as it stands low.
    let stage = |effect| FallStage {
        effects: vec![name(effect)],
        wrecks: Vec::new(),
    };
    assert_eq!(
        units.units["raptor"].death.fall,
        Some(DeathFall {
            gravity: num("0.6"),
            roll: num("1.5"),
            spiral: None,
            start: stage("fx_jet_start"),
            hit: FallStage::default(),
            blow_up_after: num("1"),
            blow_up: FallStage::default(),
            ground: Some(GroundDeath {
                height: num("0.64"),
                stage: stage("fx_jet_ground"),
            }),
        })
    );
    assert!(units.units["lamp"].death.is_empty());
    let gun = &units.actions()[&name("humvee_gun")];
    assert_eq!((&gun.fire, &gun.hit), (&Some(name("fx_gun_fire")), &None));
    let shell = &units.units["humvee_shell"];
    assert_eq!(
        (&shell.end, &shell.trail),
        (&Some(name("fx_shell_hit")), &Some(name("smoke_trail")))
    );
    let written = Toml::write(&units);
    assert_eq!(
        [
            "slot =", "node =", "start =", "middle =", "hit =", "end =", "trail ="
        ]
        .map(|field| written.matches(field).count()),
        [1, 1, 2, 1, 0, 3, 1]
    );
    // A state's sets, its own and its aliases', the last of two untested conditions.
    let damaged = DeclaredName::new("damaged").unwrap();
    let night = DeclaredName::new("night").unwrap();
    assert_eq!(
        states[1].sets,
        [
            StateSet::from([damaged.clone()]),
            StateSet::from([damaged.clone(), night]),
            StateSet::new([damaged], 2)
        ]
    );
    // A draw's ignored conditions.
    let ignore = units.units["lamp"].draws[0].ignore();
    assert_eq!(ignore, [DeclaredName::new("night").unwrap()]);
    // The state a unit draws by the conditions it holds: damaged, the damaged state; damaged at
    // night, its set of both; none, the default. The lamp ignores the night, so at night it
    // draws its default, of no condition, before its night set.
    let drawn = &units.units["humvee"].draws[0];
    let holding =
        |names: &'static [&'static str]| move |name: &DeclaredName| names.contains(&name.as_str());
    assert_eq!(drawn.best_state(holding(&["damaged"])), 1);
    assert_eq!(drawn.best_state(holding(&["damaged", "night"])), 1);
    assert_eq!(drawn.best_state(holding(&[])), 0);
    let lamp = &units.units["lamp"].draws[0];
    assert_eq!(lamp.best_state(holding(&["night"])), 0);
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
    assert_eq!(test("fallen"), ConditionTest::Dead);
    assert_eq!(test("engaged"), ConditionTest::Attacking);
    let site = |stage| ConditionTest::Site { stage };
    assert_eq!(
        [test("awaiting"), test("partly")],
        [site(SiteStage::Waiting), site(SiteStage::Started)]
    );
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
    // A draw of no state; a state of no set, or that rises by less than 0; a state of a condition
    // of no test, in a variation too; a life share whose bounds do not rise, or pass 1.
    let draws = |draws: &str| format!("[units.a]\ndraws = {draws}");
    let empty =
        "a draw has no state, a state of no set or that sinks, or a pair's transition twice";
    assert!(refusal(&draws("[{ states = [] }]")).starts_with(empty));
    assert!(refusal(&draws("[{ states = [{ sets = [], models = [] }] }]")).starts_with(empty));
    let sinks = "[{ states = [{ sets = [[]], models = [], rise = \"-1\" }] }]";
    assert!(refusal(&draws(sinks)).starts_with(empty));
    let untested = "a look's state names the condition `night`, of no test";
    let at_night = "[{ states = [{ sets = [[\"night\"]], models = [] }] }]";
    assert_eq!(refusal(&draws(at_night)), untested);
    let counted =
        "[{ states = [{ sets = [{ names = [\"night\"], untested = 1 }], models = [] }] }]";
    assert_eq!(refusal(&draws(counted)), untested);
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

#[test]
fn a_states_animation_and_a_draws_transitions_write_and_read_back_and_refuse_what_never_ends() {
    let text = r#"
        [units.ranger]
        draws = [{ states = [
            { sets = [[]], models = [], key = "stand", rise = "2.5", animation = { clips = [
                { animation = "client/animations/a.stand.glb" },
                { animation = "client/animations/a.stand.glb" },
                { animation = "client/animations/a.fidget.glb" },
            ], idle = true, mode = "once", start = "random", speed = ["0.75", "1.25"] } },
            { sets = [["moving"]], models = [], key = "walk", waits_for = "stand", animation = { clips = [
                { animation = "client/animations/a.walk.glb", stride = "2.5" },
            ], mode = "loop", maintain = 3, restart = true } },
        ], transitions = [
            { from = "stand", to = "walk", models = [], rise = "2.5", animation = { clips = [
                { animation = "client/animations/a.step.glb" },
            ], mode = "once_backwards" } },
        ] }]

        [conditions]
        moving = { test = "moving", still_ms = 250 }
    "#;
    let units: ClientUnits = Toml::parse(text).unwrap();
    assert_eq!(
        Toml::parse::<ClientUnits>(&Toml::write(&units)).unwrap(),
        units
    );
    let draw = &units.units["ranger"].draws[0];
    let [stand, walk] = [0, 1].map(|at| &draw.states()[at]);
    // Standing: three idle clips, the first named twice, once from a random frame, at a speed of
    // 0.75 to 1.25; a rise of 2.5 m as its site is built; its key, and no state it waits for.
    let two_and_a_half = Num::int(5) / Num::int(2);
    assert_eq!((stand.rise, walk.rise), (Some(two_and_a_half), None));
    let idle = stand.animation.as_ref().unwrap();
    let paths: Vec<&str> = idle
        .clips()
        .iter()
        .map(|clip| clip.animation.as_str())
        .collect();
    assert_eq!(
        paths,
        [
            "client/animations/a.stand.glb",
            "client/animations/a.stand.glb",
            "client/animations/a.fidget.glb"
        ]
    );
    assert!(idle.idle() && !idle.restart());
    assert_eq!(
        (idle.mode(), idle.start()),
        (AnimationMode::Once, StartFrame::Random)
    );
    let three_quarters = Num::from_bits(Num::ONE.to_bits() * 3 / 4);
    let five_quarters = Num::from_bits(Num::ONE.to_bits() * 5 / 4);
    assert_eq!(
        idle.speed(),
        SpeedRange::new(three_quarters, five_quarters).unwrap()
    );
    assert_eq!(
        (
            stand.key.as_ref().unwrap().as_str(),
            stand.waits_for.as_ref()
        ),
        ("stand", None)
    );
    // Walking: one clip of a stride of 2.5 m, looped from its mode's start, its frame kept across
    // the first two groups, started again as it completes, at its own rate; it lets a standing
    // state finish first.
    let walking = walk.animation.as_ref().unwrap();
    assert_eq!(walking.clips()[0].stride, Some(Num::int(5) / Num::int(2)));
    assert_eq!(
        (walking.mode(), walking.start()),
        (AnimationMode::Loop, StartFrame::Natural)
    );
    assert_eq!(
        (walking.maintain(), walking.restart(), walking.speed()),
        (3, true, SpeedRange::ONE)
    );
    assert_eq!(walk.waits_for.as_ref().unwrap().as_str(), "stand");
    // The step from standing to walking, once backwards, of the same rise.
    let step = &draw.transitions()[0];
    assert_eq!(
        (step.from().as_str(), step.to().as_str()),
        ("stand", "walk")
    );
    let shown = step.shown();
    assert_eq!(
        (shown.animation.as_ref().unwrap().mode(), shown.rise),
        (AnimationMode::OnceBackwards, Some(two_and_a_half))
    );
    // The defaults are not written.
    let written = Toml::write(&units);
    assert!(!written.contains("natural") && !written.contains("idle = false"));

    // No clip; groups past four; a stride of none; a speed of none, or out of order; a
    // transition from a key to itself, of a loop, or that rises by less than 0, or a pair's twice.
    let refusal = |text: &str| {
        Toml::parse::<ClientUnits>(text)
            .unwrap_err()
            .message()
            .to_owned()
    };
    let animated = |animation: &str| {
        format!(
            "[units.a]\ndraws = [{{ states = [{{ sets = [[]], models = [], animation = {animation} }}] }}]"
        )
    };
    let clip = r#"{ animation = "a.glb" }"#;
    let no_clip = "an animation has no clip, or groups past four";
    assert_eq!(
        refusal(&animated(r#"{ clips = [], mode = "loop" }"#)),
        no_clip
    );
    assert_eq!(
        refusal(&animated(&format!(
            "{{ clips = [{clip}], mode = \"loop\", maintain = 16 }}"
        ))),
        no_clip
    );
    let stride = r#"{ clips = [{ animation = "a.glb", stride = "0" }], mode = "loop" }"#;
    assert_eq!(refusal(&animated(stride)), "a clip's stride is not above 0");
    for speed in [r#"["0", "1"]"#, r#"["1.5", "1"]"#] {
        let text = animated(&format!(
            "{{ clips = [{clip}], mode = \"loop\", speed = {speed} }}"
        ));
        assert!(
            refusal(&text).starts_with("a speed range is not above 0 and in order"),
            "{speed}"
        );
    }
    let transitions = |transitions: &str| {
        format!(
            "[units.a]\ndraws = [{{ states = [{{ sets = [[]], models = [] }}], transitions = {transitions} }}]"
        )
    };
    let never = "a transition is from its key to itself, plays other than once, or sinks";
    let to_itself = r#"[{ from = "a", to = "a", models = [] }]"#;
    assert!(refusal(&transitions(to_itself)).starts_with(never));
    let sinks = r#"[{ from = "a", to = "b", models = [], rise = "-1" }]"#;
    assert!(refusal(&transitions(sinks)).starts_with(never));
    let looped = format!(
        "[{{ from = \"a\", to = \"b\", models = [], animation = {{ clips = [{clip}], mode = \"loop\" }} }}]"
    );
    assert!(refusal(&transitions(&looped)).starts_with(never));
    let twice = r#"[{ from = "a", to = "b", models = [] }, { from = "a", to = "b", models = [] }]"#;
    let pair = "a draw has no state, a state of no set or that sinks, or a pair's transition twice";
    assert!(refusal(&transitions(twice)).starts_with(pair));
}
