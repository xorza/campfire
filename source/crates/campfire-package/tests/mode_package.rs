//! Each flaw a package can have fails the load of the reference packages with its own problem.

use std::fmt::Write;
use std::num::NonZeroU32;
use std::path::Path;

use campfire_capabilities::{
    ActionDataField, ActionError, ActionField, ActionKind, AiError, CapabilitySet, DeclaredName,
    EffectData, EffectTo, Effecting, EngineEnum, EngineTag, Hook, MapProblem, ModeError,
    ModifierProblem, NameKind, Number, ParamProblem, PlannedEffect, Scalar, Status, SyncTo,
    UnitKitError,
};
use campfire_package::{
    BoxProblem, BuildProblem, ChoiceProblem, ContentError, CtxMisuse, DeliveryProblem,
    EffectProblem, GatherProblem, Limit, LoadError, LoadProblem, LocaleProblem, ModePackages,
    PackageRef, Place, ScriptProblem, Way,
};
use campfire_script::ScriptError;
use campfire_script::rhai::ParseErrorType;
use campfire_sim::{Capability, TickRate};

use crate::moba::{Edit, edited, moba, moba_files};

/// A flaw, the package it is in, and the problem it fails the load with.
#[derive(Debug)]
struct Flaw {
    file: &'static str,
    edit: Edit<'static>,
    /// More edits the flaw needs, each to a file.
    also: &'static [(&'static str, Edit<'static>)],
    package: &'static str,
    refused: fn(&LoadProblem) -> bool,
}

const fn flaw(
    file: &'static str,
    edit: Edit<'static>,
    package: &'static str,
    refused: fn(&LoadProblem) -> bool,
) -> Flaw {
    Flaw {
        file,
        edit,
        also: &[],
        package,
        refused,
    }
}

const MANIFEST: &str = "modes/3v3/manifest.toml";
const UNITS: &str = "modes/3v3/data/units.toml";
const MAP: &str = "modes/3v3/map/map.toml";
const HUSK: &str = "heroes/husk/data/avatar.toml";
const HUSK_MANIFEST: &str = "heroes/husk/manifest.toml";
const HUSK_TEXT: &str = "heroes/husk/locale/en.ftl";
const GALE: &str = "heroes/gale/data/avatar.toml";
const CINDER: &str = "heroes/cinder/data/avatar.toml";
const VEIL: &str = "heroes/veil/data/avatar.toml";
const RIME: &str = "heroes/rime/data/avatar.toml";
/// Rime's Fan of Frost's `on_hit` effect that slows.
const SLOWS: &str = r#"{ modifier = { id = "slow", duration_ms = { param = "slow_ms" } } },"#;
const LASH_OUT: &str = "heroes/husk/scripts/lash_out.rhai";
const CHAIN_FIRE: &str = "heroes/cinder/scripts/chain_fire.rhai";
/// The script of Rime's passive.
const STILLNESS: &str = "heroes/rime/scripts/stillness.rhai";
const SNOW_OWL: &str = "heroes/rime/scripts/snow_owl.rhai";
const CREEP_AI: &str = "modes/3v3/scripts/creep_ai.rhai";
const MODE: &str = "moba-3v3";
/// A package whose manifest does not read has no name, so its directory names it.
const MODE_DIR: &str = "modes/3v3";
const MODE_DATA: &str = "modes/3v3/data/mode.toml";
const MODE_SCRIPT: &str = "modes/3v3/scripts/mode.rhai";
/// A train of a melee creep, which the mode's data does not hold, put before its first action.
const RECRUIT: &str = "[actions.recruit]\nkind = \"train\"\ntargeting = \"none\"\nunit_type = \"melee_creep\"\n\n[actions.melee_creep_attack]";
/// A yard with the mode's life pool, put before the tower.
const LIVING_YARD: Edit<'static> = Edit::Replace(
    "[units.tower]\n",
    "[units.yard]\npools = [\"health\"]\nstats = { health = { base = 500 } }\ncombat = {}\ncollision = { box = [\"4\", \"2\"] }\n\n[units.tower]\n",
);
/// A yard, a unit type of a box of 4 by 2 m, put before the tower.
const YARD: Edit<'static> = Edit::Replace(
    "[units.tower]\n",
    "[units.yard]\ncollision = { box = [\"4\", \"2\"] }\n\n[units.tower]\n",
);
/// A gather of gold, in reach of 1 m, a trip of 5 in a second, that looks 6 m round a node gone.
const MINE: &str = "[actions.mine]\nkind = \"gather\"\nrange = \"1\"\nwindup_ms = 1000\ntargeting = \"neutrals\"\nresource = \"gold\"\ntake = 5\nbounce = \"6\"\n\n[actions.melee_creep_attack]";
/// A mine of 1500 gold, a box.
const GOLD_MINE: Edit<'static> = Edit::Replace(
    "[units.tower]\n",
    "[units.mine]\ncollision = { box = [\"2\", \"2\"] }\nnode = { resource = \"gold\", amount = 1500 }\n\n[units.tower]\n",
);
/// The manifest's capabilities with `production`.
const PRODUCTION: Edit<'static> = Edit::Replace(r#""items"]"#, r#""items", "production"]"#);

/// Whether `problem` is Rime's Slow's `slow` param failing for `kind` by `way`.
fn slow_fails(problem: &LoadProblem, way: Option<&Way>, kind: ParamProblem) -> bool {
    matches!(problem, LoadProblem::ModifierParam { modifier, param, way: found, problem } if modifier == "slow" && param == "slow" && found.as_ref() == way && *problem == kind)
}

/// Whether `problem` is a script that does not compile, as it reads the variable `name`, which
/// nothing defines before it.
fn undefined(problem: &LoadProblem, name: &str) -> bool {
    matches!(problem, LoadProblem::Script { problem: ScriptProblem::Compile(ScriptError::Compile(error)), .. } if matches!(&*error.0, ParseErrorType::VariableUndefined(found) if found == name))
}

/// The way of the action `name`.
fn by(name: &str) -> Way {
    Way::Action(DeclaredName::new(name).unwrap())
}

/// Whether `problem` is the manifest failing to read with a message that starts with `message`.
fn manifest_fails(problem: &LoadProblem, message: &str) -> bool {
    read_fails(problem, "manifest.toml", message)
}

/// Whether `problem` is the file at `file` failing to read with a message that starts with
/// `message`.
fn read_fails(problem: &LoadProblem, file: &str, message: &str) -> bool {
    matches!(problem, LoadProblem::Content(ContentError::Data { path, error })
        if path.to_string() == file && error.message().starts_with(message))
}

#[test]
fn a_caster_creep_projectile_slower_than_the_cap_fails_the_load() {
    // 5 m/s, slower than the 3v3's cap of 6: a homing projectile might never catch a hero.
    let edit = Edit::Set("units.caster_creep_bolt.projectile.speed", r#""5.0""#);
    let error = ModePackages::from_package_dir(&edited([(UNITS, edit)])).unwrap_err();
    assert_eq!(error.package, PackageRef::Name(MODE.to_owned()));
    let at_caster = |problem: &LoadProblem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::NotFaster(Place::UnitType(name))) if name == "caster_creep_bolt");
    assert!(at_caster(&error.problem), "{error:?}");
    // Along a line, the same speed loads: it chases no one.
    let edit = Edit::Set("units.grasping_wraps.projectile.speed", r#""5""#);
    assert!(ModePackages::from_package_dir(&edited([(HUSK, edit)])).is_ok());
}

#[test]
fn a_mode_state_field_is_sent_to_no_client_unless_it_says() {
    // The 3v3's `phase` is sent to all; without `sync`, or with `sync = "none"`, to none.
    let phase = |edit: Edit<'_>| {
        let packages = ModePackages::from_package_dir(&edited([(MODE_DATA, edit)])).unwrap();
        packages.data().state["phase"].sync
    };
    let pick = r#"phase = { type = "string", default = "pick", sync = "all" }"#;
    assert_eq!(phase(Edit::Replace(pick, pick)), SyncTo::All);
    let unsent = r#"phase = { type = "string", default = "pick" }"#;
    assert_eq!(phase(Edit::Replace(pick, unsent)), SyncTo::None);
    let none = r#"phase = { type = "string", default = "pick", sync = "none" }"#;
    assert_eq!(phase(Edit::Replace(pick, none)), SyncTo::None);
}

#[test]
fn an_effect_to_the_source_reads_and_any_other_to_does_not_and_a_purge_reads_its_tag() {
    // Rime's Fan of Frost's hits also heal its caster by 1: the list reads it after the slow.
    let to_source = Edit::Replace(
        SLOWS,
        r#"{ modifier = { id = "slow", duration_ms = { param = "slow_ms" } } },
    { heal = { amount = 1 }, to = "source" },"#,
    );
    let packages = ModePackages::from_package_dir(&edited([(RIME, to_source)])).unwrap();
    let rime = packages
        .dependencies()
        .iter()
        .find(|dependent| dependent.package.header.name == "hero-rime")
        .unwrap();
    let heal = EffectData {
        does: Effecting::Heal {
            amount: Number::Value(Scalar::Int(1)),
        },
        to: EffectTo::Source,
    };
    assert_eq!(
        rime.content.actions["fan_of_frost"].on_hit.last(),
        Some(&heal)
    );
    // A purge names a tag the match declares, which Rime's slow grants.
    let purge = Edit::Replace(SLOWS, r#"{ purge = { tag = "slowed" } },"#);
    let packages = ModePackages::from_package_dir(&edited([(RIME, purge)])).unwrap();
    let rime = packages
        .dependencies()
        .iter()
        .find(|dependent| dependent.package.header.name == "hero-rime")
        .unwrap();
    let purge = Effecting::Purge {
        tag: DeclaredName::new("slowed").unwrap(),
    };
    assert_eq!(
        rime.content.actions["fan_of_frost"]
            .on_hit
            .last()
            .map(|effect| &effect.does),
        Some(&purge)
    );
    // `to` names the source alone.
    let to_target = Edit::Replace(SLOWS, r#"{ heal = { amount = 1 }, to = "target" },"#);
    let error = ModePackages::from_package_dir(&edited([(RIME, to_target)])).unwrap_err();
    assert!(
        read_fails(&error.problem, "data/avatar.toml", "unknown variant"),
        "{error:?}"
    );
}

/// The end of the melee creep's weapon, which delivers at once.
const MELEE_END: &str = "damage_kind = \"physical\"\n\n[actions.caster_creep_attack]";

/// The melee creep's weapon's end with a list `list` of one true damage of its `bite` param, half
/// its attack damage plus 5.
fn biting(list: &str) -> String {
    format!(
        "damage_kind = \"physical\"\n{list} = [{{ damage = {{ amount = {{ param = \"bite\" }}, kind = \"true\" }} }}]\n\n[actions.melee_creep_attack.params]\nbite = {{ base = 5, attack_damage = \"0.5\" }}\n\n[actions.caster_creep_attack]"
    )
}

#[test]
fn a_weapon_takes_params_and_an_on_hit_list_and_refuses_an_on_end_list() {
    // A weapon's `on_hit` follows its attack, which reaches its target at once with no delivery.
    let on_hit = biting("on_hit");
    let edit = Edit::Replace(MELEE_END, &on_hit);
    let packages = ModePackages::from_package_dir(&edited([(MODE_DATA, edit)])).unwrap();
    let weapon = &packages.content().actions["melee_creep_attack"];
    assert_eq!((weapon.on_hit.len(), weapon.params.len()), (1, 1));
    let on_end = biting("on_end");
    let edit = Edit::Replace(MELEE_END, &on_end);
    let error = ModePackages::from_package_dir(&edited([(MODE_DATA, edit)])).unwrap_err();
    let refused = |problem: &LoadProblem| matches!(problem, LoadProblem::KindField { action, field: ActionDataField::OnEnd } if action == "melee_creep_attack");
    assert!(refused(&error.problem), "{error:?}");
    // Cinder's bolt launches an eruption, an area type of her own package, where it hits.
    let edit = Edit::Set(
        "actions.attack.on_hit",
        r#"[{ launch = { area = "eruption" } }]"#,
    );
    let packages = ModePackages::from_package_dir(&edited([(CINDER, edit)])).unwrap();
    let cinder = packages
        .dependencies()
        .iter()
        .find(|dependent| dependent.package.header.name == "hero-cinder")
        .unwrap();
    let launch = Effecting::Launch {
        area: DeclaredName::new("eruption").unwrap(),
        on_hit: Vec::new(),
        on_end: Vec::new(),
    };
    let on_hit = &cinder.content.actions["attack"].on_hit;
    assert_eq!(
        on_hit.iter().map(|effect| &effect.does).collect::<Vec<_>>(),
        [&launch]
    );
}

/// The 3v3 as its packages hold it.
fn three_v_three() -> ModePackages {
    ModePackages::from_dir(&moba().join(MODE_DIR)).unwrap()
}

#[test]
fn the_books_build_at_every_rate_the_manifest_allows() {
    // The 3v3 allows 20 to 60 ticks a second; the load built its books at 60.
    let packages = three_v_three();
    for hz in [20, 30, 60] {
        let rate = TickRate::new(NonZeroU32::new(hz).unwrap());
        packages.books(rate);
    }
}

#[test]
#[should_panic(expected = "a session's rate is within the manifest's range")]
fn the_books_refuse_a_rate_past_the_manifests_range() {
    let packages = three_v_three();
    let rate = TickRate::new(NonZeroU32::new(61).unwrap());
    packages.books(rate);
}

#[test]
fn a_mode_type_may_share_its_name_with_a_dependencys_delivery_type() {
    // Husk's `grasping_wraps` is in Husk's own scope, the mode's `grasping_wraps` in the
    // mode's, so both load.
    let edits = [(
        UNITS,
        Edit::Replace(
            "[units.tower_bolt]\n",
            "[units.grasping_wraps]\nprojectile = { speed = \"20\" }\n\n[units.tower_bolt]\n",
        ),
    )];
    let packages = ModePackages::from_package_dir(&edited(edits)).unwrap();
    assert!(packages.content().units.contains_key("grasping_wraps"));
}

#[test]
fn a_mode_script_spawns_its_own_unit_types_and_avatars_by_name() {
    // The mode's tower, and Husk by its package's name, are in the mode's scope; the team is one
    // of the manifest's.
    let spawns = r#"    share_xp(ctx, unit);
    ctx.spawn_unit("tower", "north", unit.pos);
    ctx.spawn_unit("hero-husk", "camps", unit.pos);
"#;
    let edit = Edit::Replace("    share_xp(ctx, unit);\n", spawns);
    assert!(ModePackages::from_package_dir(&edited([(MODE_SCRIPT, edit)])).is_ok());
}

#[test]
fn the_engines_tags_and_the_modes_fill_the_tags_a_match_holds() {
    // The 3v3 names 25 tags: the 10 of its `[tags]`, 6 of its heroes' classes, 8 more of its unit
    // types', `ward` among them, and `slowed`, which modifiers grant. The engine has 7, so 224
    // layers, each a tag, fill the 256 a match holds, and 225 are past it.
    let tags = |packages: &ModePackages| packages.tag_names().len();
    let packages = ModePackages::from_package_dir(&edited([])).unwrap();
    assert_eq!(tags(&packages), 25);
    // Each once, in order, as a match declares them: `ward` after `slowed`.
    assert!(packages.tag_names().is_sorted_by(|a, b| a < b));
    let at = |name: &str| {
        packages
            .tag_names()
            .iter()
            .position(|tag| tag.as_str() == name)
    };
    assert!(at("slowed").unwrap() < at("ward").unwrap());
    let [(path, layers)] = <[_; 1]>::try_from(layers(224)).unwrap();
    let edit = Edit::Set(&path, &layers);
    let packages = ModePackages::from_package_dir(&edited([(MODE_DATA, edit)])).unwrap();
    assert_eq!(tags(&packages), 249);
    // The mode's `[tags]` may give an engine tag properties, and that names no tag of its own.
    let edit = Edit::Replace(
        "[tags.stunned]",
        "[tags.projectile]\nhidden = true\n\n[tags.stunned]",
    );
    let packages = ModePackages::from_package_dir(&edited([(MODE_DATA, edit)])).unwrap();
    assert_eq!(tags(&packages), 25);
}

/// `count` navigation layers, each a tag: the edit by path that declares them.
fn layers(count: usize) -> Vec<(String, String)> {
    let names: Vec<String> = (0..count).map(|at| format!("\"layer{at}\"")).collect();
    vec![(
        "navigation.layers".to_owned(),
        format!("[{}]", names.join(", ")),
    )]
}

/// `count` tracks beside `level`: the edits by path that declare them.
fn tracks(count: usize) -> Vec<(String, String)> {
    (0..count)
        .map(|at| (format!("tracks.skill{at}"), "{ levels = [10] }".to_owned()))
        .collect()
}

/// A limit of the load: `more(count)` gives the edits that declare `count` more of what it
/// counts, of which `allowed` load.
#[derive(Debug)]
struct LimitCase {
    more: fn(usize) -> Vec<(String, String)>,
    allowed: usize,
    limit: Limit,
}

#[test]
fn a_mode_loads_up_to_each_limit_and_fails_one_past_it() {
    let cases = [
        // The 3v3's 25 tags and the engine's 7 leave 224 of the 256 a match holds for layers.
        LimitCase {
            more: layers,
            allowed: 224,
            limit: Limit::Tags,
        },
        // `level` and 31 more fill the 32 tracks a unit holds.
        LimitCase {
            more: tracks,
            allowed: 31,
            limit: Limit::Tracks,
        },
    ];
    for case in cases {
        let load = |count| {
            let sets = (case.more)(count);
            let edits = sets
                .iter()
                .map(|(path, value)| (MODE_DATA, Edit::Set(path, value)));
            ModePackages::from_package_dir(&edited(edits))
        };
        assert!(load(case.allowed).is_ok(), "{case:?}");
        let error = load(case.allowed + 1).unwrap_err();
        assert_eq!(error.package, PackageRef::Name(MODE.to_owned()));
        assert!(
            matches!(*error.problem, LoadProblem::TooMany(limit) if limit == case.limit),
            "{case:?}: {error:?}"
        );
    }
}

/// Each flaw, one to a copy of the packages, and the problem it fails the load with.
static FLAWS: [Flaw; 270] = [
    // The release runs package API 1.0: another major, and a newer minor, do not load.
    flaw(
        MANIFEST,
        Edit::Set("api", r#""2.0""#),
        MODE,
        |problem| matches!(problem, LoadProblem::OtherApi(api) if api.to_string() == "2.0"),
    ),
    flaw(
        MANIFEST,
        Edit::Set("api", r#""1.0.0""#),
        MODE_DIR,
        |problem| manifest_fails(problem, r#""1.0.0" is not major.minor"#),
    ),
    flaw(
        "heroes/husk/manifest.toml",
        Edit::Set("api", r#""1.1""#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::OtherApi(api) if api.to_string() == "1.1"),
    ),
    flaw(
        MANIFEST,
        Edit::Replace(r#"["combat","#, r#"["mode", "combat","#),
        MODE_DIR,
        |problem| manifest_fails(problem, "declares mode, which every match has"),
    ),
    flaw(
        MANIFEST,
        Edit::Replace(r#"["combat","#, r#"["combat", "combat","#),
        MODE_DIR,
        |problem| manifest_fails(problem, "declares Combat twice"),
    ),
    flaw(
        MANIFEST,
        Edit::Replace(r#", "navigation""#, ""),
        MODE_DIR,
        |problem| manifest_fails(problem, "declares Orders without Navigation"),
    ),
    // A section of the mode's data or of its map is its capability's, declared or refused: the
    // map's vision grid without `vision`, its pathing grid without `navigation`, `[supply]`
    // without `production`.
    flaw(
        MANIFEST,
        Edit::Replace(r#", "vision", "progression""#, r#", "progression""#),
        MODE,
        |problem| {
            matches!(
                problem,
                LoadProblem::Undeclared {
                    capability: Capability::Vision,
                    at: Place::MapGrid
                }
            )
        },
    ),
    flaw(
        MANIFEST,
        Edit::Replace(r#", "orders", "navigation""#, ""),
        MODE,
        |problem| {
            matches!(
                problem,
                LoadProblem::Undeclared {
                    capability: Capability::Navigation,
                    at: Place::MapNavigation
                }
            )
        },
    ),
    flaw(
        MODE_DATA,
        Edit::Set("supply", "{ max = 10 }"),
        MODE,
        |problem| {
            matches!(
                problem,
                LoadProblem::Undeclared {
                    capability: Capability::Production,
                    at: Place::Supply
                }
            )
        },
    ),
    // Tracks are progression's: positive and ascending, at most one the `level` track, and each a
    // unit type lists one the mode declares.
    flaw(
        MANIFEST,
        Edit::Replace(r#", "progression", "items"]"#, r#", "items"]"#),
        MODE,
        |problem| {
            matches!(
                problem,
                LoadProblem::Undeclared {
                    capability: Capability::Progression,
                    at: Place::Tracks
                }
            )
        },
    ),
    flaw(
        MODE_DATA,
        Edit::Set("tracks.level.levels", "[660, 280]"),
        MODE,
        |problem| read_fails(problem, "data/mode.toml", "a track has a level 2"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            "[tracks.level]\n",
            "[tracks.fame]\nlevel = true\nlevels = [10]\n\n[tracks.level]\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::LevelTracks),
    ),
    // A slot kind's levels: one for each of its ranks, each at least 1 and at least the one
    // before, on a kind with ranks, in a mode with the `level` track.
    flaw(
        MODE_DATA,
        Edit::Replace("levels = [6, 11, 16]", "levels = [6, 11]"),
        MODE,
        |problem| {
            read_fails(
                problem,
                "data/mode.toml",
                "a slot kind's `levels` gives one level",
            )
        },
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("levels = [6, 11, 16]", "levels = [6, 16, 11]"),
        MODE,
        |problem| {
            read_fails(
                problem,
                "data/mode.toml",
                "a slot kind's `levels` gives one level",
            )
        },
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("levels = [1, 3, 5, 7, 9]", "levels = [0, 3, 5, 7, 9]"),
        MODE,
        |problem| read_fails(problem, "data/mode.toml", "a level is at least 1"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("name = \"spell\"\n", "name = \"spell\"\nlevels = [1]\n"),
        MODE,
        |problem| {
            read_fails(
                problem,
                "data/mode.toml",
                "a slot kind's `levels` needs its `ranks`",
            )
        },
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("level = true\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::RankLevels(kind) if kind.as_str() == "basic"),
    ),
    flaw(
        HUSK,
        Edit::Replace(r#"tracks = ["level"]"#, r#"tracks = ["levels"]"#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Track, name, .. } if name == "levels"),
    ),
    flaw(
        MANIFEST,
        Edit::Set("tick_hz", "{ min = 40, max = 60, default = 30 }"),
        MODE_DIR,
        |problem| manifest_fails(problem, "the tick rate range does not hold its default"),
    ),
    // A call of no operation, which Rhai would read as no limit.
    flaw(
        MANIFEST,
        Edit::Set(
            "script_limits",
            "{ per_call = 0, player = 40000, think = 200000, mode = 100000 }",
        ),
        MODE_DIR,
        |problem| {
            manifest_fails(
                problem,
                "invalid value: integer `0`, expected a nonzero u64",
            )
        },
    ),
    // A player's pool below the whole call of 20 000.
    flaw(
        MANIFEST,
        Edit::Set(
            "script_limits",
            "{ per_call = 20000, player = 19999, think = 200000, mode = 100000 }",
        ),
        MODE_DIR,
        |problem| manifest_fails(problem, "a script pool holds less than a whole call"),
    ),
    flaw(
        MANIFEST,
        Edit::Set(
            "script_limits",
            "{ per_call = 20000, player = 40000, think = 200000, mode = 19999 }",
        ),
        MODE_DIR,
        |problem| manifest_fails(problem, "a script pool holds less than a whole call"),
    ),
    flaw(
        MANIFEST,
        Edit::Set("max_move_speed", r#""0""#),
        MODE_DIR,
        |problem| manifest_fails(problem, "a speed is a positive number"),
    ),
    flaw(
        MANIFEST,
        Edit::Replace("hero-husk = {", "hero-hask = {"),
        "hero-hask",
        |problem| matches!(problem, LoadProblem::OtherName(name) if name == "hero-husk"),
    ),
    flaw(
        HUSK,
        Edit::Replace(r#""tomb_bind"]"#, r#""tomb_binds"]"#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::UnknownSlot(id) if id == "tomb_binds"),
    ),
    flaw(
        HUSK,
        Edit::Set(
            "actions.grasping_wraps.cooldown_ms",
            "[16000, 14000, 12000, 10000]",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::RankCount { action, ranks: 5 } if action == "grasping_wraps"),
    ),
    flaw(
        HUSK,
        Edit::Replace(
            "[combat]\n",
            "orders = { ai = \"scripts/lash_out.rhai\", think_ms = 250 }\n[combat]\n",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::AvatarOrders),
    ),
    flaw(
        HUSK,
        Edit::Replace("[combat]\n", "[fight]\n"),
        "hero-husk",
        |problem| read_fails(problem, "data/avatar.toml", "unknown field `fight`"),
    ),
    // An action delivers a projectile type of its own package, which homes only alone and at a
    // unit, and needs an aim; a weapon's homes. A projectile type is a delivery type alone, a
    // dependency's unit types are all delivery types, and only actions make projectiles.
    flaw(
        HUSK,
        Edit::Set("units.grasping_wraps.projectile.homing", "true"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::Homing(action)) if action == "grasping_wraps"),
    ),
    flaw(
        HUSK,
        Edit::Replace(
            r#"delivery = { projectile = "grasping_wraps" }"#,
            r#"delivery = { projectile = "wraps" }"#,
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::UnitType, at: Place::Action(action), name } if action == "grasping_wraps" && name == "wraps"),
    ),
    flaw(
        HUSK,
        Edit::Set("actions.grasping_wraps.targeting", r#""none""#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::NoAim(action)) if action == "grasping_wraps"),
    ),
    flaw(
        HUSK,
        Edit::Replace(
            "[combat]\n",
            "projectile = { speed = \"20\" }\n\n[combat]\n",
        ),
        "hero-husk",
        |problem| {
            matches!(
                problem,
                LoadProblem::Delivery(DeliveryProblem::NotDelivery(Place::Avatar(_)))
            )
        },
    ),
    flaw(
        UNITS,
        Edit::Replace(
            "[units.tower_bolt]\n",
            "[units.tower_bolt]\npools = [\"health\"]\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::NotDelivery(Place::UnitType(name))) if name == "tower_bolt"),
    ),
    flaw(
        UNITS,
        Edit::Remove("units.tower_bolt.projectile.homing"),
        MODE,
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::Weapon(action)) if action == "tower_attack"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            r#"delivery = { projectile = "tower_bolt" }"#,
            r#"delivery = { projectile = "tower" }"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::WrongSection { action, unit_type }) if action == "tower_attack" && unit_type == "tower"),
    ),
    // An area delivery names an area type, which lands on a point, a unit or the caster, takes
    // no count, and holds modifiers of its package; a delivery type has one section of the two.
    flaw(
        CINDER,
        Edit::Replace(
            r#"delivery = { area = "eruption" }"#,
            r#"delivery = { area = "fire_lance" }"#,
        ),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::WrongSection { action, unit_type }) if action == "eruption" && unit_type == "fire_lance"),
    ),
    flaw(
        CINDER,
        Edit::Set("actions.eruption.targeting", r#""direction""#),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::AreaDirection(action)) if action == "eruption"),
    ),
    flaw(
        CINDER,
        Edit::Replace(
            r#"delivery = { area = "eruption" }"#,
            r#"delivery = { area = "eruption", count = 2 }"#,
        ),
        "hero-cinder",
        |problem| read_fails(problem, "data/avatar.toml", "an area takes no count"),
    ),
    flaw(
        CINDER,
        Edit::Set("units.eruption.area.affects", r#""enemies:molten""#),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Filter, at: Place::UnitType(name), .. } if name == "eruption"),
    ),
    flaw(
        CINDER,
        Edit::Replace(
            "[units.eruption]\n",
            "[units.eruption]\nprojectile = { speed = \"20\" }\n",
        ),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::NotDelivery(Place::UnitType(name))) if name == "eruption"),
    ),
    flaw(
        VEIL,
        Edit::Replace(r#"self = "smoke_ring_cover""#, r#"self = "smoke_cover""#),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Modifier, at: Place::UnitType(unit_type), name } if unit_type == "smoke_ring" && name == "smoke_cover"),
    ),
    flaw(
        MANIFEST,
        Edit::Replace(r#""areas", "#, ""),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Undeclared { capability: Capability::Areas, at: Place::UnitType(name) } if name == "eruption"),
    ),
    flaw(
        MAP,
        Edit::Replace("unit_type = \"tower\"", "unit_type = \"tower_bolt\""),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::UnknownUnitType(name)) if name == "tower_bolt"),
    ),
    // Snow Owl hits nothing: a width, a stop on hit, a hit once a cast or homing would use hits,
    // and so would an `on_hit` list or hook of its action.
    flaw(
        RIME,
        Edit::Set("units.snow_owl.projectile.width", r#""0.5""#),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::HitsNothing(Place::UnitType(name))) if name == "snow_owl"),
    ),
    flaw(
        RIME,
        Edit::Set("units.snow_owl.projectile.stop_on_hit", "true"),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::HitsNothing(Place::UnitType(name))) if name == "snow_owl"),
    ),
    flaw(
        RIME,
        Edit::Set("units.snow_owl.projectile.once_per_cast", "true"),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::HitsNothing(Place::UnitType(name))) if name == "snow_owl"),
    ),
    flaw(
        RIME,
        Edit::Set("units.snow_owl.projectile.homing", "true"),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::HitsNothing(Place::UnitType(name))) if name == "snow_owl"),
    ),
    flaw(
        RIME,
        Edit::Set(
            "actions.snow_owl.on_hit",
            r#"[{ damage = { amount = 1, kind = "true" } }]"#,
        ),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::NoHit(action)) if action == "snow_owl"),
    ),
    flaw(
        SNOW_OWL,
        Edit::Replace(
            "fn on_end(ctx, caster, hit) {",
            "fn on_hit(ctx, caster, target, hit) {}\n\nfn on_end(ctx, caster, hit) {",
        ),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::NoHit(action)) if action == "snow_owl"),
    ),
    flaw(
        RIME,
        Edit::Set("units.snow_owl.projectile.hits", r#""nobody""#),
        "hero-rime",
        |problem| read_fails(problem, "data/avatar.toml", r#"filter "nobody""#),
    ),
    // A launch names an area type of its own package, and its `on_end` reaches no unit.
    flaw(
        CINDER,
        Edit::Set(
            "actions.attack.on_hit",
            r#"[{ launch = { area = "fire_lance" } }]"#,
        ),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::WrongSection { action, unit_type }) if action == "attack" && unit_type == "fire_lance"),
    ),
    flaw(
        CINDER,
        Edit::Set(
            "actions.attack.on_hit",
            r#"[{ launch = { area = "pyre" } }]"#,
        ),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::UnitType, at: Place::Action(action), name } if action == "attack" && name == "pyre"),
    ),
    flaw(
        CINDER,
        Edit::Set(
            "actions.attack.on_hit",
            r#"[{ launch = { area = "eruption", on_end = [{ heal = { amount = 1 } }] } }]"#,
        ),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Effect { action, list: Hook::OnEnd, problem: EffectProblem::NoUnit } if action == "attack"),
    ),
    // An action spawns a unit type of its own package that stands, an avatar's summon among
    // them, for a whole number of milliseconds: Rime's arrow, a projectile, is none. A summon of
    // Rime's names only what the mode declares, and one of the spells' loadout slots its actions
    // only in kinds of the loadout's ranks. Loot is planned.
    flaw(
        RIME,
        Edit::Replace(SLOWS, r#"{ spawn = { unit_type = "frost_arrow" } },"#),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::UnitType, name, .. } if name == "frost_arrow"),
    ),
    flaw(
        RIME,
        Edit::Set("units.wisp.stats.nonsense.base", "1"),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Stat, name, .. } if name == "nonsense"),
    ),
    flaw(
        "spells/data/loadout.toml",
        Edit::Set("units.totem.slots.basic", r#"["blink"]"#),
        "player-spells",
        |problem| matches!(problem, LoadProblem::ActionRanks(action) if action.as_str() == "blink"),
    ),
    flaw(
        RIME,
        Edit::Replace(SLOWS, r#"{ loot = { table = "chest", level = 1 } },"#),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Effect { action, list: Hook::OnHit, problem: EffectProblem::Planned(PlannedEffect::Loot) } if action == "fan_of_frost"),
    ),
    flaw(
        MODE_DATA,
        Edit::Set(
            "actions.tower_attack.on_hit",
            r#"[{ spawn = { unit_type = "caster_creep_bolt" } }]"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::UnitType, name, .. } if name == "caster_creep_bolt"),
    ),
    flaw(
        MODE_DATA,
        Edit::Set(
            "actions.tower_attack.on_hit",
            r#"[{ spawn = { unit_type = "melee_creep", duration_ms = "1.5" } }]"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Effect { action, list: Hook::OnHit, problem: EffectProblem::Duration } if action == "tower_attack"),
    ),
    // A move is a dash's `to` and `speed`, or a knock back's `from`, `distance` and whole `ms`;
    // its other unit, as its own, is one its list reaches.
    flaw(
        RIME,
        Edit::Replace(SLOWS, r#"{ move = { to = "source", distance = 1 } },"#),
        "hero-rime",
        |problem| read_fails(problem, "data/avatar.toml", "a dash's"),
    ),
    flaw(
        RIME,
        Edit::Replace(
            SLOWS,
            r#"{ move = { from = "source", distance = 1, ms = "0.5" } },"#,
        ),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Effect { action, list: Hook::OnHit, problem: EffectProblem::Duration } if action == "fan_of_frost"),
    ),
    flaw(
        CINDER,
        Edit::Set(
            "actions.attack.on_hit",
            r#"[{ launch = { area = "eruption", on_end = [{ move = { to = "reached", speed = 5 }, to = "source" }] } }]"#,
        ),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Effect { action, list: Hook::OnEnd, problem: EffectProblem::NoUnit } if action == "attack"),
    ),
    flaw(
        RIME,
        Edit::Replace(SLOWS, r#"{ purge = { tag = "frozen" } },"#),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Tag, at: Place::Action(action), name } if action == "fan_of_frost" && name == "frozen"),
    ),
    flaw(
        RIME,
        Edit::Replace(SLOWS, r#"{ purge = { tag = "avatar" } },"#),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::EngineTag { at: Place::Action(action), tag: EngineTag::Avatar } if action == "fan_of_frost"),
    ),
    flaw(
        RIME,
        Edit::Replace(
            SLOWS,
            r#"{ heal = { amount = 1 }, restore = { pool = "mana", amount = 1 } },"#,
        ),
        "hero-rime",
        |problem| read_fails(problem, "data/avatar.toml", "exactly one effect"),
    ),
    flaw(
        RIME,
        Edit::Replace("on_hit = [", "on_end = ["),
        "hero-rime",
        |problem| {
            matches!(
                problem,
                LoadProblem::Effect {
                    list: Hook::OnEnd,
                    problem: EffectProblem::NoUnit,
                    ..
                }
            )
        },
    ),
    flaw(
        RIME,
        Edit::Replace("on_hit = [", "on_resolve = ["),
        "hero-rime",
        |problem| {
            matches!(
                problem,
                LoadProblem::Effect {
                    list: Hook::OnResolve,
                    problem: EffectProblem::NoUnit,
                    ..
                }
            )
        },
    ),
    flaw(
        RIME,
        Edit::Remove("actions.fan_of_frost.delivery"),
        "hero-rime",
        |problem| {
            matches!(
                problem,
                LoadProblem::Effect {
                    list: Hook::OnHit,
                    problem: EffectProblem::NoDelivery,
                    ..
                }
            )
        },
    ),
    flaw(
        RIME,
        Edit::Set(
            "actions.fan_of_frost.params.damage.base",
            "[40, 50, -60, 70, 80]",
        ),
        "hero-rime",
        |problem| {
            matches!(
                problem,
                LoadProblem::Effect {
                    problem: EffectProblem::Negative,
                    ..
                }
            )
        },
    ),
    flaw(
        RIME,
        Edit::Set("actions.fan_of_frost.params.slow_ms", r#""2000.5""#),
        "hero-rime",
        |problem| {
            matches!(
                problem,
                LoadProblem::Effect {
                    problem: EffectProblem::Duration,
                    ..
                }
            )
        },
    ),
    flaw(
        RIME,
        Edit::Replace(r#"kind = "physical" } }"#, r#"kind = "frost" } }"#),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::DamageKind, name, .. } if name == "frost"),
    ),
    flaw(
        RIME,
        Edit::Replace(
            r#"id = "slow", duration_ms"#,
            r#"id = "frozen", duration_ms"#,
        ),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Modifier, name, .. } if name == "frozen"),
    ),
    flaw(
        MANIFEST,
        Edit::Replace(r#""projectiles", "#, ""),
        MODE,
        |problem| matches!(problem, LoadProblem::Undeclared { capability: Capability::Projectiles, at: Place::UnitType(name) } if name == "caster_creep_bolt"),
    ),
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.recruit]\nkind = \"train\"\ntargeting = \"none\"\nunit_type = \"tower_bolt\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::Trained(action)) if action == "recruit"),
    },
    flaw(
        "heroes/husk/scripts/extra.rhai",
        Edit::Create("fn on_resolve(ctx, caster, target) {}"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { path, problem: ScriptProblem::Unreferenced } if path.to_string() == "scripts/extra.rhai"),
    ),
    flaw(
        HUSK,
        Edit::Replace("scripts/dread.rhai", "scripts/dreadful.rhai"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { path, problem: ScriptProblem::Missing } if path.to_string() == "scripts/dreadful.rhai"),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace("fn on_damage_taken(", "fn on_damage_takn("),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownHook(function), .. } if function == "on_damage_takn"),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace("fn on_damage_taken(", "fn calc_damage_taken("),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownHook(function), .. } if function == "calc_damage_taken"),
    ),
    flaw(
        CREEP_AI,
        Edit::Replace(
            "fn on_think(ctx, unit) {",
            "fn on_resolve(ctx, caster, target) {}\n\nfn on_think(ctx, unit) {",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownHook(function), .. } if function == "on_resolve"),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace("fn on_resolve(", "fn on_cast("),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownHook(function), .. } if function == "on_cast"),
    ),
    // A handle's member every form of which is of a capability the mode does not declare: the
    // 3v3 has no `production`, whose `unit.load` it is.
    flaw(
        CREEP_AI,
        Edit::Replace(
            "if target == () {\n        ctx.order_follow_path",
            "if unit.load > 0 {\n        ctx.order_follow_path",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Undeclared { capability: Capability::Production, at: Place::Script(path) } if path.as_str() == "scripts/creep_ai.rhai"),
    ),
    flaw(
        CREEP_AI,
        Edit::Replace("fn on_think(ctx, unit)", "fn on_think(ctx, unit, more)"),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownHook(function), .. } if function == "on_think"),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace("    for unit in", "    let c = ctx;\n    for unit in"),
        "hero-husk",
        |problem| {
            matches!(
                problem,
                LoadProblem::Script {
                    problem: ScriptProblem::CtxMisuse(CtxMisuse::Stray),
                    ..
                }
            )
        },
    ),
    // A unit type's state field has a default of its type, and a script names after `.state`
    // only a field some state of the match declares.
    flaw(
        CINDER,
        Edit::Set("units.chain_fire.state.bounces_left.default", r#""full""#),
        "hero-cinder",
        |problem| read_fails(problem, "data/avatar.toml", "a default not of the type"),
    ),
    flaw(
        CHAIN_FIRE,
        Edit::Replace(
            "hit.delivery.state.bounces_left == 0",
            "hit.delivery.state.bounces == 0",
        ),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownState(name), .. } if name == "bounces"),
    ),
    // A variable that nothing defines before its use fails the load, in a hook and in a
    // function of the script alike.
    flaw(
        LASH_OUT,
        Edit::Replace(
            r#"ctx.damage(unit, ctx.p.damage, "magic");"#,
            r#"ctx.damage(unti, ctx.p.damage, "magic");"#,
        ),
        "hero-husk",
        |problem| undefined(problem, "unti"),
    ),
    flaw(
        CREEP_AI,
        Edit::Replace("let target = unit.target;", "let target = unti.target;"),
        MODE,
        |problem| undefined(problem, "unti"),
    ),
    flaw(
        CREEP_AI,
        Edit::Replace(
            "fn defend_hero(ctx, unit) {\n    for ally in ctx.find(",
            "fn defend_hero(c, unit) {\n    for ally in c.find(",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::CtxMisuse(CtxMisuse::Renamed { function }), .. } if function == "defend_hero"),
    ),
    // An engine enum's argument takes a member, never a string literal; a member and a function
    // are the enum's own; a module that is no engine enum's fails the compile.
    flaw(
        MODE_SCRIPT,
        Edit::Replace("lane_end(ctx, team), wave", r#""start", wave"#),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::EnumString { call, takes: EngineEnum::PathEnd }, .. } if call == "spawn_group"),
    ),
    flaw(
        MODE_SCRIPT,
        Edit::Replace(
            "PathEnd::named(spawn_marker(ctx, team).params.from)",
            "PathEnd::Middle",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownEnumMember { path, of: EngineEnum::PathEnd }, .. } if path == "PathEnd::Middle"),
    ),
    flaw(
        MODE_SCRIPT,
        Edit::Replace("PathEnd::named(spawn_marker", "PathEnd::name(spawn_marker"),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownEnumMember { path, of: EngineEnum::PathEnd }, .. } if path == "PathEnd::name"),
    ),
    flaw(
        MODE_SCRIPT,
        Edit::Replace("PathEnd::named(spawn_marker", "Path::named(spawn_marker"),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::Compile(ScriptError::Compile(error)), .. } if matches!(&*error.0, ParseErrorType::ModuleUndefined(module) if module == "Path")),
    ),
    // No script makes a function pointer: a closure, an anonymous function that captures
    // nothing, or `Fn`.
    flaw(
        LASH_OUT,
        Edit::Replace(
            r#"    for unit in ctx.find(caster, caster.pos, ctx.p.radius, "enemies") {"#,
            r#"    let radius = ctx.p.radius;
    let near = |unit| unit.pos.within(caster.pos, radius);
    for unit in ctx.find(caster, caster.pos, radius, "enemies") {"#,
        ),
        "hero-husk",
        |problem| {
            matches!(
                problem,
                LoadProblem::Script {
                    problem: ScriptProblem::FunctionPointer,
                    ..
                }
            )
        },
    ),
    flaw(
        LASH_OUT,
        Edit::Replace(
            r#"    for unit in ctx.find(caster, caster.pos, ctx.p.radius, "enemies") {"#,
            r#"    let near = |unit| unit.pos;
    for unit in ctx.find(caster, caster.pos, ctx.p.radius, "enemies") {"#,
        ),
        "hero-husk",
        |problem| {
            matches!(
                problem,
                LoadProblem::Script {
                    problem: ScriptProblem::FunctionPointer,
                    ..
                }
            )
        },
    ),
    flaw(
        LASH_OUT,
        Edit::Replace(
            r#"    for unit in ctx.find(caster, caster.pos, ctx.p.radius, "enemies") {"#,
            r#"    let hook = Fn("on_resolve");
    for unit in ctx.find(caster, caster.pos, ctx.p.radius, "enemies") {"#,
        ),
        "hero-husk",
        |problem| {
            matches!(
                problem,
                LoadProblem::Script {
                    problem: ScriptProblem::FunctionPointer,
                    ..
                }
            )
        },
    ),
    flaw(
        LASH_OUT,
        Edit::Replace(
            "fn on_damage_taken(ctx, m, d)",
            "fn on_damage_taken(m, ctx, d)",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::CtxMisuse(CtxMisuse::HookParam { function }), .. } if function == "on_damage_taken"),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace("    for unit in", "    let ctx = 1;\n    for unit in"),
        "hero-husk",
        |problem| {
            matches!(
                problem,
                LoadProblem::Script {
                    problem: ScriptProblem::CtxMisuse(CtxMisuse::Bound),
                    ..
                }
            )
        },
    ),
    flaw(
        LASH_OUT,
        Edit::Replace("caster.pos", "caster.position"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownMember(name), .. } if name == "position"),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace(
            "    for unit in",
            "    ctx.spawn_avatars();\n    for unit in",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownCtx(name), .. } if name == "spawn_avatars"),
    ),
    // A weapon has a rate, a damage and a damage kind the mode declares, aims at a unit within
    // meters, and has no field only a cast runs; no other kind has a weapon's fields.
    flaw(
        MODE_DATA,
        Edit::Replace(
            "damage_kind = \"physical\"\ndelivery = { projectile = \"tower_bolt\" }",
            "damage_kind = \"fire\"\ndelivery = { projectile = \"tower_bolt\" }",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::DamageKind, at: Place::Action(action), name: kind } if action == "tower_attack" && kind == "fire"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("rate = \"attack_speed\"\n", "rate = \"attack_sped\"\n"),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Stat, at: Place::Action(action), name } if action == "melee_creep_attack" && name == "attack_sped"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("rate = \"attack_speed\"\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::KindField { action, field: ActionDataField::Rate } if action == "melee_creep_attack"),
    ),
    flaw(
        MODE_DATA,
        Edit::Set("actions.tower_attack.range", r#""global""#),
        MODE,
        |problem| matches!(problem, LoadProblem::GlobalAttack(action) if action == "tower_attack"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            "[actions.tower_attack]\nkind = \"attack\"\ntargeting = \"enemies\"",
            "[actions.tower_attack]\nkind = \"attack\"\ntargeting = \"point\"",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::AttackAims(action) if action == "tower_attack"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            "[actions.wolf_attack]\n",
            "[actions.wolf_attack]\ncooldown_ms = 1000\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::KindField { action, field: ActionDataField::CooldownMs } if action == "wolf_attack"),
    ),
    flaw(
        HUSK,
        Edit::Replace(
            "[actions.dread]\n",
            "[actions.dread]\nrate = \"attack_speed\"\n",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::KindField { action, field: ActionDataField::Rate } if action == "dread"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            "damage_kinds = [\"physical\", \"magic\", \"true\"]",
            "damage_kinds = []",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::NoDamageKinds),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            r#"heal_scale = "healing_received_pct""#,
            r#"heal_scale = "heal_taken""#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Stat, at: Place::Combat, name } if name == "heal_taken"),
    ),
    flaw(MAP, Edit::Remove("grid"), MODE, |problem| {
        matches!(problem, LoadProblem::NoGrid)
    }),
    flaw(MAP, Edit::Remove("navigation"), MODE, |problem| {
        matches!(problem, LoadProblem::NoPathingGrid)
    }),
    flaw(MAP, Edit::Set("grid.cell", r#""0""#), MODE, |problem| {
        matches!(problem, LoadProblem::Mode(ModeError::Grid))
    }),
    flaw(
        MAP,
        Edit::Set("bounds", "{ min = [1, 0], max = [0, 1] }"),
        MODE,
        |problem| read_fails(problem, "map/map.toml", "bounds need min below max"),
    ),
    flaw(MAP, Edit::Set("units.0.pos", "[0, -69]"), MODE, |problem| {
        matches!(problem, LoadProblem::Mode(ModeError::OutOfBounds))
    }),
    // A meter from the west lane's waypoint 1, at (−36, −36), less than the tower's body and the
    // widest walker's together.
    flaw(
        MAP,
        Edit::Set("units.2.pos", "[-36, -37]"),
        MODE,
        |problem| {
            matches!(problem, LoadProblem::Map(MapProblem::WaypointBlocked { path, waypoint })
                if path == "west" && *waypoint == 1)
        },
    ),
    // A meter from the north spawn, at (0, −60).
    flaw(MAP, Edit::Set("units.2.pos", "[0, -59]"), MODE, |problem| {
        matches!(problem, LoadProblem::Map(MapProblem::MarkerBlocked { marker })
                if marker == "north_spawn")
    }),
    flaw(
        UNITS,
        Edit::Set("units.melee_creep.vision.sight_range", r#""-1.0""#),
        MODE,
        |problem| {
            read_fails(
                problem,
                "data/units.toml",
                "a sight range is a number that is not negative",
            )
        },
    ),
    flaw(
        HUSK,
        Edit::Set("actions.lash_out.cost.mana", "-35"),
        "hero-husk",
        |problem| {
            matches!(
                problem,
                LoadProblem::ActionField {
                    field: ActionField::Cost,
                    ..
                }
            )
        },
    ),
    Flaw {
        file: UNITS,
        edit: Edit::Replace("[units.melee_creep]", "[units.husk]\n\n[units.melee_creep]"),
        also: &[
            (MANIFEST, Edit::Replace("hero-husk = {", "husk = {")),
            (
                HUSK_MANIFEST,
                Edit::Replace(r#"name = "hero-husk""#, r#"name = "husk""#),
            ),
        ],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Repeated { at: Place::UnitTypes, name } if name == "husk"),
    },
    flaw(
        LASH_OUT,
        Edit::Replace("ctx.find(", "ctx.finds("),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownCtx(name), .. } if name == "finds"),
    ),
    // An AI's order in the mode's script.
    flaw(
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(
            "fn on_match_start(ctx) {",
            "fn on_match_start(ctx) {\n    ctx.order_reset(());",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownCtx(name), .. } if name == "order_reset"),
    ),
    // Taking a bot's slot is a late join.
    flaw(
        MODE_DATA,
        Edit::Replace("late_join = true", "late_join = false"),
        MODE,
        |problem| matches!(problem, LoadProblem::BotTakeoverWithoutLateJoin),
    ),
    // Names design 08 plans: a `ctx` method and field, a handle's field, a hook and data fields.
    flaw(
        MODE_SCRIPT,
        Edit::Replace(
            "fn on_match_start(ctx) {",
            "fn on_match_start(ctx) {\n    ctx.generate(\"north\");",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::Planned(name), .. } if name == "generate"),
    ),
    flaw(
        MODE_SCRIPT,
        Edit::Replace(
            "fn on_match_start(ctx) {",
            "fn on_match_start(ctx) {\n    let carried = ctx.carry;",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::Planned(name), .. } if name == "carry"),
    ),
    flaw(
        CHAIN_FIRE,
        Edit::Replace(
            "if hit.delivery.state",
            "if hit.part == () && hit.delivery.state",
        ),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::Planned(name), .. } if name == "part"),
    ),
    flaw(
        MODE_SCRIPT,
        Edit::Replace(
            "fn on_match_start(ctx) {",
            "fn on_generate(ctx, region) {}\n\nfn on_match_start(ctx) {",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::Planned(name), .. } if name == "on_generate"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            "resources = [\"gold\"]",
            "resources = [\"gold\"]\nstate_version = 1",
        ),
        MODE,
        |problem| {
            matches!(
                problem,
                LoadProblem::Planned {
                    field: "state_version",
                    at: Place::Mode
                }
            )
        },
    ),
    flaw(
        UNITS,
        Edit::Replace(
            "speed = \"12\", homing = true",
            "speed = \"12\", homing = true, gravity = \"9.8\"",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Planned { field: "gravity", at: Place::UnitType(name) } if name == "tower_bolt"),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace("ctx.p.radius", "ctx.p.radios"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Param, at: Place::Script(path), name } if path.to_string() == "scripts/lash_out.rhai" && name == "radios"),
    ),
    // A cooldown cut names an ability of the script's own package.
    flaw(
        LASH_OUT,
        Edit::Replace(
            r#"ctx.reduce_cooldown(m.carrier, "lash_out""#,
            r#"ctx.reduce_cooldown(m.carrier, "lash""#,
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Ability, at: Place::Script(path), name } if path.to_string() == "scripts/lash_out.rhai" && name == "lash"),
    ),
    flaw(
        "heroes/cinder/scripts/wildfire.rhai",
        Edit::Replace(r#""kindle""#, r#""kindl""#),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Modifier, name: id, .. } if id == "kindl"),
    ),
    flaw(
        "heroes/veil/scripts/dual_path.rhai",
        Edit::Replace(r#"stat("attack_damage")"#, r#"stat("attack_dmg")"#),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Stat, name, .. } if name == "attack_dmg"),
    ),
    flaw(
        CREEP_AI,
        Edit::Replace("enemies:creep", "enemies:minion"),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Filter, name: filter, .. } if filter == "enemies:minion"),
    ),
    flaw(
        HUSK,
        Edit::Rename("stats.magic_resist", "spirit"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Stat, name, .. } if name == "spirit"),
    ),
    flaw(
        HUSK,
        Edit::Rename("modifiers.withered.stats.magic_resist", "spirit"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Stat, name, .. } if name == "spirit"),
    ),
    // A hold needs a toggle or a channel to hold its modifier while.
    flaw(
        RIME,
        Edit::Remove("actions.chill_arrows.toggle"),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::HoldAlone(action) if action.as_str() == "chill_arrows"),
    ),
    // Only a point aim clamps to the range.
    flaw(
        "spells/data/loadout.toml",
        Edit::Set("actions.blink.targeting", r#""direction""#),
        "player-spells",
        |problem| matches!(problem, LoadProblem::ClampAims(action) if action.as_str() == "blink"),
    ),
    // A stat change names one operation; a scaling param scales with declared stats alone.
    flaw(
        "spells/data/loadout.toml",
        Edit::Set("modifiers.haste.stats.move_speed.cut", r#""0.1""#),
        "player-spells",
        |problem| {
            read_fails(
                problem,
                "data/loadout.toml",
                "a stat change is a number, or one of",
            )
        },
    ),
    flaw(
        HUSK,
        Edit::Rename(
            "actions.grasping_wraps.params.damage.ability_power",
            "spell_power",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Stat, at: Place::Action(_), name } if name == "spell_power"),
    ),
    // The mode's `surge` adds attack damage by a param that only the mode's action `surge`,
    // which applies it, declares, and that reads spell vamp; `vamp` adds spell vamp by its own
    // param, which reads attack damage: a loop that only the mode's actions show.
    flaw(
        MODE_DATA,
        Edit::Replace(
            "[actions.melee_creep_attack]",
            "[modifiers.surge]\nstats = { attack_damage = { param = \"surge\" } }\n\n[modifiers.vamp]\nstats = { spell_vamp = { param = \"vamp\" } }\n[modifiers.vamp.params]\nvamp = { base = 0, attack_damage = \"1\" }\n\n[actions.surge]\ntargeting = \"none\"\npassive_modifier = \"surge\"\n[actions.surge.params]\nsurge = { base = 0, spell_vamp = \"10\" }\n\n[actions.melee_creep_attack]",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::StatLoop(stats) if stats.iter().map(ToString::to_string).eq(["attack_damage", "spell_vamp"])),
    ),
    // Dual Path's spell vamp reads attack damage; a modifier whose attack damage reads spell
    // vamp closes a loop.
    flaw(
        "heroes/veil/data/avatar.toml",
        Edit::Replace(
            "[modifiers.dual_path.params]",
            "[modifiers.loop]\nstats = { attack_damage = { param = \"thirst\" } }\n[modifiers.loop.params]\nthirst = { base = 0, spell_vamp = \"10\" }\n\n[modifiers.dual_path.params]",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::StatLoop(stats) if stats.iter().map(ToString::to_string).eq(["attack_damage", "spell_vamp"])),
    ),
    flaw(
        "heroes/veil/data/avatar.toml",
        Edit::Replace("bonus = { attack_damage =", "bonus = { attack_dmg ="),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Stat, at: Place::Modifier(id), name } if id == "dual_path" && name == "attack_dmg"),
    ),
    // Pools: each a unit lists, a cost names or a script reads is declared, and the life pool is
    // among a combatant's; no pool shares a name with a player resource, and no more than eight.
    flaw(
        HUSK,
        Edit::Replace(
            r#"pools = ["health", "mana"]"#,
            r#"pools = ["health", "rage"]"#,
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Pool, at: Place::Avatar(_), name } if name == "rage"),
    ),
    flaw(
        HUSK,
        Edit::Replace(
            r#"pools = ["health", "mana"]"#,
            r#"pools = ["health", "mana", "mana"]"#,
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Repeated { at: Place::Avatar(_), name } if name == "mana"),
    ),
    flaw(
        HUSK,
        Edit::Replace(r#"pools = ["health", "mana"]"#, r#"pools = ["mana"]"#),
        "hero-husk",
        |problem| {
            matches!(
                problem,
                LoadProblem::UnitKit {
                    at: Place::Avatar(_),
                    error: UnitKitError::NoLifePool
                }
            )
        },
    ),
    flaw(
        UNITS,
        Edit::Replace("pools = [\"health\"]\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::UnitKit { at: Place::UnitType(name), error: UnitKitError::NoLifePool } if name == "melee_creep"),
    ),
    flaw(
        UNITS,
        Edit::Replace("combat = { on_death = \"stay\" }\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::CombatMissing(Place::UnitType(name)) if name == "inhibitor"),
    ),
    flaw(
        UNITS,
        Edit::Replace(r#""tower", "true_sight"]"#, r#""tower", "avatar"]"#),
        MODE,
        |problem| matches!(problem, LoadProblem::EngineTag { at: Place::UnitType(name), tag: EngineTag::Avatar } if name == "tower"),
    ),
    flaw(
        HUSK,
        Edit::Replace(r#"tags = ["tank"]"#, r#"tags = ["area"]"#),
        "hero-husk",
        |problem| {
            matches!(
                problem,
                LoadProblem::EngineTag {
                    at: Place::Avatar(_),
                    tag: EngineTag::Area
                }
            )
        },
    ),
    flaw(
        RIME,
        Edit::Replace(r#"tags = ["slowed"]"#, r#"tags = ["projectile"]"#),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::EngineTag { at: Place::Modifier(id), tag: EngineTag::Projectile } if id == "slow"),
    ),
    flaw(
        HUSK,
        Edit::Replace(r#"tags = ["tank"]"#, r#"tags = ["tank:front"]"#),
        "hero-husk",
        |problem| read_fails(problem, "data/avatar.toml", r#""tank:front" is not a name"#),
    ),
    flaw(
        "heroes/kensho/data/avatar.toml",
        Edit::Replace(r#"left = { type = "int""#, r#"Left = { type = "int""#),
        "hero-kensho",
        |problem| read_fails(problem, "data/avatar.toml", r#""Left" is not a name"#),
    ),
    // What a match start refused before, now at load: a unit type that makes no unit, an AI
    // that does not load, and a time that does not count in ticks at the fastest rate, 60 Hz.
    // 4 × 10¹⁷ ms counts at the default 30 Hz, 1.2 × 10¹⁹ ticks, and not at 60 Hz, past 2⁶⁴.
    flaw(
        UNITS,
        Edit::Set("units.melee_creep.stats.health.base", "0"),
        MODE,
        |problem| matches!(problem, LoadProblem::UnitKit { at: Place::UnitType(name), error: UnitKitError::NotPositive(_) } if name == "melee_creep"),
    ),
    flaw(
        UNITS,
        Edit::Set("units.melee_creep.orders.think_ms", "400000000000000000"),
        MODE,
        |problem| {
            matches!(
                problem,
                LoadProblem::Ai {
                    error: AiError::TimeTooLarge,
                    ..
                }
            )
        },
    ),
    flaw(
        CREEP_AI,
        Edit::Replace("fn on_think(ctx, unit) {", "fn think(ctx, unit) {"),
        MODE,
        |problem| {
            matches!(
                problem,
                LoadProblem::Ai {
                    at: Place::UnitType(_),
                    error: AiError::NoThink
                }
            )
        },
    ),
    flaw(
        HUSK,
        Edit::Set("actions.lash_out.cooldown_ms.4", "400000000000000000"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Action { action, error: ActionError::TimeTooLarge } if action.as_str() == "lash_out"),
    ),
    flaw(
        HUSK,
        Edit::Set("modifiers.withered.duration_ms", "-1"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Modifier { modifier, problem: ModifierProblem::Time } if modifier == "withered"),
    ),
    // A stat change past what a number holds: 9 × 10¹² is past 2³⁹, as a number has 24 bits
    // of fraction in 64.
    flaw(
        HUSK,
        Edit::Set("modifiers.withered.stats.magic_resist", "-9000000000000"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Modifier { modifier, problem: ModifierProblem::Overflow } if modifier == "withered"),
    ),
    flaw(
        CINDER,
        Edit::Set("units.eruption.area.delay_ms", "400000000000000000"),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::AreaTime(name)) if name == "eruption"),
    ),
    // An avatar package named as its own delivery type: the area's time names the delivery type.
    Flaw {
        file: CINDER,
        edit: Edit::Set("units.eruption.area.delay_ms", "400000000000000000"),
        also: &[
            (MANIFEST, Edit::Replace("hero-cinder = {", "eruption = {")),
            (
                "heroes/cinder/manifest.toml",
                Edit::Replace(r#"name = "hero-cinder""#, r#"name = "eruption""#),
            ),
        ],
        package: "eruption",
        refused: |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::AreaTime(name)) if name == "eruption"),
    },
    // A modifier is the passive of one owner of its package: Husk's avatar holds
    // `withering_touch`, and Lash Out `lash_out_guard`.
    flaw(
        HUSK,
        Edit::Set("actions.lash_out.passive_modifier", r#""withering_touch""#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::SharedPassive { modifier, owners: [Place::Avatar(avatar), Place::Action(action)] } if modifier == "withering_touch" && avatar == "hero-husk" && action == "lash_out"),
    ),
    flaw(
        HUSK,
        Edit::Set("actions.dread.passive_modifier", r#""lash_out_guard""#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::SharedPassive { modifier, owners: [Place::Action(first), Place::Action(second)] } if modifier == "lash_out_guard" && first == "dread" && second == "lash_out"),
    ),
    // The same of the mode, a unit type's and an action's, and of a loadout, two actions'.
    Flaw {
        file: UNITS,
        edit: Edit::Set("units.tower.passive", r#""warden_blessing""#),
        also: &[(
            MODE_DATA,
            Edit::Set(
                "actions.tower_attack.passive_modifier",
                r#""warden_blessing""#,
            ),
        )],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::SharedPassive { modifier, owners: [Place::UnitType(tower), Place::Action(action)] } if modifier == "warden_blessing" && tower == "tower" && action == "tower_attack"),
    },
    Flaw {
        file: "spells/data/loadout.toml",
        edit: Edit::Set("actions.blink.passive_modifier", r#""haste""#),
        also: &[(
            "spells/data/loadout.toml",
            Edit::Set("actions.haste.passive_modifier", r#""haste""#),
        )],
        package: "player-spells",
        refused: |problem| matches!(problem, LoadProblem::SharedPassive { modifier, owners: [Place::Action(first), Place::Action(second)] } if modifier == "haste" && first == "blink" && second == "haste"),
    },
    // An aura's radius and a shield are never negative: as Gale's Fair Wind's value, as a rank
    // of the Wind Shield action's param its modifier reads, or as the modifier's own param.
    flaw(
        GALE,
        Edit::Set("modifiers.fair_wind.aura.radius", r#""-1.0""#),
        "hero-gale",
        |problem| matches!(problem, LoadProblem::Modifier { modifier, problem: ModifierProblem::Negative } if modifier == "fair_wind"),
    ),
    flaw(
        GALE,
        Edit::Set(
            "actions.wind_shield.params.shield.base",
            "[80, 120, -1, 200, 240]",
        ),
        "hero-gale",
        |problem| matches!(problem, LoadProblem::Modifier { modifier, problem: ModifierProblem::Negative } if modifier == "wind_shield"),
    ),
    flaw(
        GALE,
        Edit::Set("modifiers.wind_shield.params.shield", "-5"),
        "hero-gale",
        |problem| matches!(problem, LoadProblem::Modifier { modifier, problem: ModifierProblem::Negative } if modifier == "wind_shield"),
    ),
    // A way that applies a modifier gives each param it reads and does not declare: Rime's Slow
    // reads `slow` of Fan of Frost, which applies it on hit; of Chill Arrows, whose held
    // modifier's script applies it; of Snow Owl, whose passive's aura applies it; and of no
    // action, with which her own passive's script applies it.
    flaw(
        RIME,
        Edit::Remove("actions.fan_of_frost.params.slow"),
        "hero-rime",
        |problem| slow_fails(problem, Some(&by("fan_of_frost")), ParamProblem::Missing),
    ),
    flaw(
        RIME,
        Edit::Remove("actions.chill_arrows.params.slow"),
        "hero-rime",
        |problem| slow_fails(problem, Some(&by("chill_arrows")), ParamProblem::Missing),
    ),
    flaw(
        RIME,
        Edit::Set(
            "modifiers.snow_owl_bounty.aura",
            r#"{ radius = "1.0", affects = "enemies", modifier = "slow" }"#,
        ),
        "hero-rime",
        |problem| slow_fails(problem, Some(&by("snow_owl")), ParamProblem::Missing),
    ),
    flaw(
        STILLNESS,
        Edit::Replace(
            "m.stacks += 1;",
            "m.stacks += 1;\n    ctx.add_modifier(m.carrier, \"slow\");",
        ),
        "hero-rime",
        |problem| slow_fails(problem, Some(&Way::NoAction), ParamProblem::Missing),
    ),
    // Its own per-rank param has a value at each rank of each action that applies it, and a
    // time reads no scaling param, whose value only the source knows as it applies.
    flaw(
        RIME,
        Edit::Set("modifiers.slow.params.slow", r#"["0.1", "0.2", "0.3"]"#),
        "hero-rime",
        |problem| slow_fails(problem, Some(&by("chill_arrows")), ParamProblem::Short),
    ),
    Flaw {
        file: RIME,
        edit: Edit::Set("modifiers.stun.duration_ms", r#"{ param = "stun_ms" }"#),
        also: &[(
            RIME,
            Edit::Set(
                "modifiers.stun.params.stun_ms",
                r#"{ base = 1000, ability_power = "1.0" }"#,
            ),
        )],
        package: "hero-rime",
        refused: |problem| matches!(problem, LoadProblem::ModifierParam { modifier, param, way: None, problem: ParamProblem::ScalingTime } if modifier == "stun" && param == "stun_ms"),
    },
    // A name a script gives the API, as the registry marks the argument: a tag, a track, a unit
    // type of the mode's scope, a team.
    flaw(
        MODE_SCRIPT,
        Edit::Replace(r#"unit.has_tag("core")"#, r#"unit.has_tag("cor")"#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Tag, name, .. } if name == "cor"),
    ),
    flaw(
        MODE_SCRIPT,
        Edit::Replace(
            r#"ctx.units_tagged("inhibitor")"#,
            r#"ctx.units_tagged("inhibitors")"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Tag, name, .. } if name == "inhibitors"),
    ),
    flaw(
        MODE_SCRIPT,
        Edit::Replace(
            r#"ctx.add_xp(hero, "level""#,
            r#"ctx.add_xp(hero, "levels""#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Track, name, .. } if name == "levels"),
    ),
    flaw(
        MODE_SCRIPT,
        Edit::Replace(
            "    share_xp(ctx, unit);\n",
            "    share_xp(ctx, unit);\n    ctx.spawn_unit(\"grasping_wraps\", \"north\", unit.pos);\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::UnitType, name, .. } if name == "grasping_wraps"),
    ),
    flaw(
        MODE_SCRIPT,
        Edit::Replace(
            "    share_xp(ctx, unit);\n",
            "    share_xp(ctx, unit);\n    ctx.spawn_unit(\"hero-husk\", \"west\", unit.pos);\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Team, name, .. } if name == "west"),
    ),
    // Human text: each message an avatar names has a value in its own language's file, and each
    // file under `locale/` is `<language>.ftl`, parses, defines no message twice, and in another
    // language defines only the own file's messages.
    flaw(
        HUSK,
        Edit::Replace(r#"name = "hero-name""#, r#"name = "hero-nam""#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Message, at: Place::Avatar(package), name } if package == "hero-husk" && name == "hero-nam"),
    ),
    flaw(
        HUSK,
        Edit::Replace(r#"name = "hero-name""#, r#"name = "hero name""#),
        "hero-husk",
        |problem| {
            read_fails(
                problem,
                "data/avatar.toml",
                r#""hero name" is not a message id"#,
            )
        },
    ),
    flaw(
        HUSK_TEXT,
        Edit::Replace("hero-name = Husk", "hero-name =\n    .short = Hu"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Message, name, .. } if name == "hero-name"),
    ),
    flaw(
        HUSK_TEXT,
        Edit::Replace("hero-name = Husk", "hero-name Husk"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Locale { path, problem: LocaleProblem::Parse(_) } if path.as_str() == "locale/en.ftl"),
    ),
    flaw(
        HUSK_TEXT,
        Edit::Replace("hero-name = Husk", "hero-name = Husk\nhero-name = Hulk"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Locale { problem: LocaleProblem::Repeated(id), .. } if id.as_str() == "hero-name"),
    ),
    flaw(
        "heroes/husk/locale/de.ftl",
        Edit::Create("hero-nme = Hülse\n"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Locale { path, problem: LocaleProblem::Stray(id) } if path.as_str() == "locale/de.ftl" && id.as_str() == "hero-nme"),
    ),
    flaw(
        "heroes/husk/locale/de_DE.ftl",
        Edit::Create("hero-name = Hülse\n"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Locale { path, problem: LocaleProblem::FileName } if path.as_str() == "locale/de_DE.ftl"),
    ),
    flaw(
        HUSK_MANIFEST,
        Edit::Replace(r#"language = "en""#, r#"language = "EN""#),
        "hero-husk",
        |problem| manifest_fails(problem, r#""EN" is not a language identifier"#),
    ),
    // 2⁴⁰ is past the 2³⁹ a number holds.
    flaw(
        HUSK,
        Edit::Set("stats.health.base", "1099511627776"),
        "hero-husk",
        |problem| {
            read_fails(
                problem,
                "data/avatar.toml",
                "Int(1099511627776) is past what a number holds",
            )
        },
    ),
    flaw(
        HUSK,
        Edit::Set("stats.health.per_level", "1099511627776"),
        "hero-husk",
        |problem| {
            read_fails(
                problem,
                "data/avatar.toml",
                "Int(1099511627776) is past what a number holds",
            )
        },
    ),
    flaw(
        "heroes/veil/data/avatar.toml",
        Edit::Set(
            "modifiers.dual_path.params.spell_vamp.bonus.attack_damage",
            "1099511627776",
        ),
        "hero-veil",
        |problem| {
            read_fails(
                problem,
                "data/avatar.toml",
                "data did not match any variant of untagged enum Param",
            )
        },
    ),
    flaw(
        HUSK,
        Edit::Replace("[actions.lash_out]", "[actions.lash-out]"),
        "hero-husk",
        |problem| read_fails(problem, "data/avatar.toml", r#""lash-out" is not a name"#),
    ),
    flaw(
        RIME,
        Edit::Replace(r#"{ param = "slow_ms" }"#, r#"{ param = "slow ms" }"#),
        "hero-rime",
        |problem| {
            read_fails(
                problem,
                "data/avatar.toml",
                "data did not match any variant of untagged enum Number",
            )
        },
    ),
    flaw(
        RIME,
        Edit::Replace(r#"hits = "enemies:avatar""#, r#"hits = "enemies:Avatar""#),
        "hero-rime",
        |problem| read_fails(problem, "data/avatar.toml", r#"filter "enemies:Avatar""#),
    ),
    flaw(
        MANIFEST,
        Edit::Replace(r#"{ name = "north""#, r#"{ name = "North""#),
        MODE_DIR,
        |problem| manifest_fails(problem, r#""North" is not a name"#),
    ),
    flaw(
        MAP,
        Edit::Replace(r#"path = "west""#, r#"path = "west lane""#),
        MODE,
        |problem| read_fails(problem, "map/map.toml", r#""west lane" is not a name"#),
    ),
    flaw(
        MODE_DATA,
        Edit::Rename("params.income_ms", "income-ms"),
        MODE,
        |problem| read_fails(problem, "data/mode.toml", r#""income-ms" is not a name"#),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("[tags.stunned]", "[tags.Stunned]"),
        MODE,
        |problem| read_fails(problem, "data/mode.toml", r#""Stunned" is not a name"#),
    ),
    flaw(
        HUSK,
        Edit::Rename("actions.lash_out.cost.mana", "rage"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Cost, at: Place::Action(id), name } if id == "lash_out" && name == "rage"),
    ),
    flaw(
        "heroes/veil/scripts/dusk_mark.rhai",
        Edit::Replace(r#""energy""#, r#""rage""#),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Pool, at: Place::Script(_), name } if name == "rage"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("life = \"health\"\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::NoLifePool),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(r#"life = "health""#, r#"life = "hp""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Pool, at: Place::Combat, name } if name == "hp"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("[pools.mana]\nmax = \"mana\"", "[pools.mana]\nmax = \"mp\""),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Stat, at: Place::Pool(pool), name } if pool.as_str() == "mana" && name == "mp"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            "[pools.energy]",
            "[pools.p1]\nmax = \"mana\"\n[pools.p2]\nmax = \"mana\"\n[pools.p3]\nmax = \"mana\"\n[pools.p4]\nmax = \"mana\"\n[pools.p5]\nmax = \"mana\"\n[pools.p6]\nmax = \"mana\"\n[pools.energy]",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::TooMany(Limit::Pools)),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(r#"resources = ["gold"]"#, r#"resources = ["gold", "gold"]"#),
        MODE,
        |problem| matches!(problem, LoadProblem::Repeated { at: Place::Resources, name } if name == "gold"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(r#"resources = ["gold"]"#, r#"resources = ["gold", "mana"]"#),
        MODE,
        |problem| matches!(problem, LoadProblem::Repeated { at: Place::Resources, name } if name == "mana"),
    ),
    // An engine stat is declared like any other: one the units carry and the mode does not
    // declare fails.
    flaw(
        MODE_DATA,
        Edit::Replace("move_speed = {}\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Stat, name, .. } if name == "move_speed"),
    ),
    flaw(
        MODE_DATA,
        Edit::Set("stats.cooldown_reduction", r#"{ min = 1, max = "0.4" }"#),
        MODE,
        |problem| read_fails(problem, "data/mode.toml", "a stat's min passes its max"),
    ),
    flaw(
        "heroes/veil/scripts/whirling_blades.rhai",
        Edit::Replace(r#""physical""#, r#""fire""#),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::DamageKind, name: kind, .. } if kind == "fire"),
    ),
    flaw(
        GALE,
        Edit::Replace(
            r#"{ param = "bonus_speed" }"#,
            r#"{ param = "bonus_speeds" }"#,
        ),
        "hero-gale",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Param, at: Place::Modifier(id), name } if id == "gust_speed" && name == "bonus_speeds"),
    ),
    flaw(
        "heroes/kensho/data/avatar.toml",
        Edit::Replace(r#"hold = "still_mind""#, r#"hold = "still_mindful""#),
        "hero-kensho",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Modifier, at: Place::Action(ability), name: id } if ability == "still_mind" && id == "still_mindful"),
    ),
    flaw(
        GALE,
        Edit::Replace(r#"affects = "allies""#, r#"affects = "allies:friends""#),
        "hero-gale",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Filter, name: filter, .. } if filter == "allies:friends"),
    ),
    flaw(
        "heroes/rime/data/avatar.toml",
        Edit::Replace(r#"hits = "enemies:avatar""#, r#"hits = "foes:avatar""#),
        "hero-rime",
        |problem| read_fails(problem, "data/avatar.toml", "filter \"foes:avatar\""),
    ),
    flaw(
        "heroes/kensho/data/avatar.toml",
        Edit::Set("modifiers.flicker_strike.state.left.sync", r#""all""#),
        "hero-kensho",
        |problem| read_fails(problem, "data/avatar.toml", "unknown field `sync`"),
    ),
    flaw(
        HUSK,
        Edit::Replace(r#""dread", "lash_out""#, r#""grasping_wraps", "lash_out""#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Repeated { at: Place::Avatar(_), name } if name == "grasping_wraps"),
    ),
    flaw(
        MAP,
        Edit::Replace(r#"unit_type = "core""#, r#"unit_type = "cor""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::UnknownUnitType(name)) if name == "cor"),
    ),
    flaw(
        MAP,
        Edit::Replace(r#"path = "west""#, r#"path = "north""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::UnknownPath(name)) if name == "north"),
    ),
    // A marker tag a script names is some marker's; a point fits the map's metric; a region is a
    // box within the bounds, of a marker with no point; a placed unit walks only a path it
    // names; marker names differ.
    flaw(
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(r#"ctx.map.markers("camp")"#, r#"ctx.map.markers("camps")"#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::MarkerTag, name: tag, .. } if tag == "camps"),
    ),
    flaw(
        MAP,
        Edit::Set("markers.0.pos", "[0, 0, -60]"),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::PointShape)),
    ),
    Flaw {
        file: MAP,
        edit: Edit::Remove("markers.4.pos"),
        also: &[(
            MAP,
            Edit::Set("markers.4.region", "{ min = [-20, -14], max = [-16, -70] }"),
        )],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Mode(ModeError::Region(marker)) if marker == "camp1"),
    },
    flaw(
        MAP,
        Edit::Set("markers.4.region", "{ min = [-20, -14], max = [-16, -10] }"),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::Region(marker)) if marker == "camp1"),
    ),
    flaw(
        MAP,
        Edit::Set("units.2.path", r#""middle""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::UnknownPath(path)) if path == "middle"),
    ),
    flaw(
        MAP,
        Edit::Set("units.0.from", r#""start""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::NoPathToWalk(unit_type)) if unit_type == "core"),
    ),
    flaw(
        MAP,
        Edit::Set("markers.5.name", r#""camp1""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::RepeatedName(name)) if name == "camp1"),
    ),
    // The layers have names of their own, a unit type moves on one of them, and they are
    // navigation's.
    flaw(
        MODE_DATA,
        Edit::Set("navigation.layers", r#"["ground", "ground"]"#),
        MODE,
        |problem| matches!(problem, LoadProblem::Repeated { at: Place::Navigation, name } if name == "ground"),
    ),
    flaw(
        UNITS,
        Edit::Set("units.melee_creep.collision.layer", r#""air""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Layer, at: Place::UnitType(unit_type), name: layer } if unit_type == "melee_creep" && layer.as_str() == "air"),
    ),
    Flaw {
        file: MANIFEST,
        edit: Edit::Replace(r#""orders", "navigation", "vision""#, r#""vision""#),
        also: &[(MODE_DATA, Edit::Set("navigation.layers", r#"["ground"]"#))],
        package: MODE,
        refused: |problem| {
            matches!(
                problem,
                LoadProblem::Undeclared {
                    capability: Capability::Navigation,
                    at: Place::Navigation
                }
            )
        },
    },
    // Slot kinds have names of their own; a unit type fills only kinds the mode declares, with
    // each of its abilities once; a choice of loadout entries fills a kind, and only one; all
    // such kinds have one count of ranks; scripts name only the mode's choices and kinds.
    flaw(
        HUSK,
        Edit::Replace(
            r#"ultimate = ["tomb_bind"]"#,
            r#"ultimates = ["tomb_bind"]"#,
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Choice(ChoiceProblem::UnknownSlotKind { at: Place::Avatar(name), kind }) if name == "hero-husk" && kind == "ultimates"),
    ),
    flaw(
        HUSK,
        Edit::Replace(
            r#""grasping_wraps", "dread", "lash_out""#,
            r#""grasping_wraps", "lash_out""#,
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unslotted(id) if id == "dread"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("name = \"ultimate\"", "name = \"basic\""),
        MODE,
        |problem| matches!(problem, LoadProblem::Repeated { at: Place::SlotKinds, name } if name == "basic"),
    ),
    flaw(
        MODE_DATA,
        Edit::Remove("choices.spells.slot"),
        MODE,
        |problem| matches!(problem, LoadProblem::Choice(ChoiceProblem::ChoiceSlot(name)) if name.as_str() == "spells"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            "unique = true\n\n[choices.spells]",
            "unique = true\nslot = \"spell\"\n\n[choices.spells]",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Choice(ChoiceProblem::ChoiceSlot(name)) if name.as_str() == "hero"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("slot = \"spell\"", "slot = \"spells\""),
        MODE,
        |problem| matches!(problem, LoadProblem::Choice(ChoiceProblem::UnknownSlotKind { at: Place::Choice(name), kind }) if name.as_str() == "spells" && kind == "spells"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            "slot = \"spell\"",
            "slot = \"spell\"\n\n[choices.more]\noffers = \"loadout\"\nslot = \"basic\"",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Choice(ChoiceProblem::LoadoutRanks)),
    ),
    flaw(
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(
            r#"ctx.chosen(player, "spells")"#,
            r#"ctx.chosen(player, "spell")"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Choice(ChoiceProblem::UnknownChoice { name, .. }) if name == "spell"),
    ),
    flaw(
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(
            r#"ctx.grant(unit, "spell","#,
            r#"ctx.grant(unit, "spells","#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Choice(ChoiceProblem::UnknownSlotKind { kind, .. }) if kind == "spells"),
    ),
    // A train is production's, names a unit type of the mode and takes no target, and sits on a
    // unit type with a queue; no other kind names a unit type.
    flaw(
        MODE_DATA,
        Edit::Replace("[actions.melee_creep_attack]", RECRUIT),
        MODE,
        |problem| matches!(problem, LoadProblem::Undeclared { capability: Capability::Production, at: Place::Action(action) } if action == "recruit"),
    ),
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace("[actions.melee_creep_attack]", RECRUIT),
        also: &[
            (MANIFEST, PRODUCTION),
            (
                MODE_DATA,
                Edit::Replace(r#"unit_type = "melee_creep""#, r#"unit_type = "knight""#),
            ),
        ],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::UnitType, name, .. } if name == "knight"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace("[actions.melee_creep_attack]", RECRUIT),
        also: &[
            (MANIFEST, PRODUCTION),
            (
                MODE_DATA,
                Edit::Replace(
                    "targeting = \"none\"\nunit_type",
                    "targeting = \"enemies\"\nunit_type",
                ),
            ),
        ],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::TrainAims(action) if action == "recruit"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace("[actions.melee_creep_attack]", RECRUIT),
        also: &[
            (MANIFEST, PRODUCTION),
            (
                UNITS,
                Edit::Replace(
                    r#"weapon = ["tower_attack"]"#,
                    r#"weapon = ["tower_attack", "recruit"]"#,
                ),
            ),
        ],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::NoQueue(action) if action == "recruit"),
    },
    // A box on a creep, which walks; on a ward, which an effect spawns; and a train of a tower,
    // which does not walk.
    flaw(
        UNITS,
        Edit::Replace(
            r#"collision = { radius = "0.35" }"#,
            r#"collision = { box = ["1", "1"] }"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::BoxBody { at: Place::UnitType(name), problem: BoxProblem::Walks } if name == "melee_creep"),
    ),
    flaw(
        UNITS,
        Edit::Replace(
            r#"tags = ["ward", "stealthed"]"#,
            "tags = [\"ward\", \"stealthed\"]\ncollision = { box = [\"0.5\", \"0.5\"] }",
        ),
        MODE,
        |problem| {
            matches!(
                problem,
                LoadProblem::Effect {
                    problem: EffectProblem::SpawnBox,
                    ..
                }
            )
        },
    ),
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace("[actions.melee_creep_attack]", RECRUIT),
        also: &[
            (MANIFEST, PRODUCTION),
            (
                MODE_DATA,
                Edit::Replace(r#"unit_type = "melee_creep""#, r#"unit_type = "tower""#),
            ),
        ],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::TrainStands(action) if action == "recruit"),
    },
    // A train's `requires` names unit types of its package that stand, and modifiers of its
    // package; a cast takes none. A unit type's `supply` needs the mode's `[supply]`.
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.recruit]\nkind = \"train\"\ntargeting = \"none\"\nunit_type = \"melee_creep\"\nrequires = { units = [\"tower_bolt\"] }\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::UnitType, at: Place::Action(action), name } if action == "recruit" && name == "tower_bolt"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.recruit]\nkind = \"train\"\ntargeting = \"none\"\nunit_type = \"melee_creep\"\nrequires = { units = [\"tower\"], modifiers = [\"drill\"] }\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Modifier, at: Place::Action(action), name } if action == "recruit" && name == "drill"),
    },
    flaw(
        HUSK,
        Edit::Replace("[actions.dread]\n", "[actions.dread]\nrequires = {}\n"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::KindField { action, field: ActionDataField::Requires } if action == "dread"),
    ),
    Flaw {
        file: UNITS,
        edit: Edit::Replace(
            r#"slots = { weapon = ["tower_attack"] }"#,
            "slots = { weapon = [\"tower_attack\"] }\nsupply = { provides = 10 }",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::SupplyUncounted(Place::UnitType(name)) if name == "tower"),
    },
    // A build aims at a point, within meters, of a unit type of its package with a box body;
    // its site starts with a share of its life above 0 when the type has the life pool; its
    // construct is needed, and its placement's filters name the mode's tags.
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"none\"\nunit_type = \"yard\"\nconstruct = \"alone\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Build(BuildProblem::Aims(action)) if action == "raise"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"tower\"\nconstruct = \"alone\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Build(BuildProblem::NoBox(action)) if action == "raise"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"dragon\"\nconstruct = \"alone\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::UnitType, name, .. } if name == "dragon"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"yard\"\nconstruct = \"alone\"\nstart_life = \"0\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Build(BuildProblem::StartLife(action)) if action == "raise"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"yard\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::KindField { action, field: ActionDataField::Construct } if action == "raise"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"yard\"\nconstruct = \"alone\"\nplacement = { near = [{ filter = \"allies:pylon\", distance = \"4\" }] }\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| {
            matches!(
                problem,
                LoadProblem::Unknown {
                    of: NameKind::Filter,
                    ..
                }
            )
        },
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"yard\"\nconstruct = \"alone\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, LIVING_YARD)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Build(BuildProblem::StartLife(action)) if action == "raise"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"global\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"yard\"\nconstruct = \"alone\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Build(BuildProblem::Global(action)) if action == "raise"),
    },
    // A gather aims at a unit its filter selects, within meters; it gathers a player resource
    // of the mode's, and its bounce is a distance; a node and a drop-off name the mode's
    // resources.
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.mine]\nkind = \"gather\"\nrange = \"1\"\nwindup_ms = 1000\ntargeting = \"point\"\nresource = \"gold\"\ntake = 5\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, GOLD_MINE)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Gather(GatherProblem::Aims(action)) if action == "mine"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.mine]\nkind = \"gather\"\nrange = \"global\"\nwindup_ms = 1000\ntargeting = \"neutrals\"\nresource = \"gold\"\ntake = 5\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, GOLD_MINE)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Gather(GatherProblem::Global(action)) if action == "mine"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.mine]\nkind = \"gather\"\nrange = \"1\"\nwindup_ms = 1000\ntargeting = \"neutrals\"\nresource = \"gold\"\ntake = 5\nbounce = \"-1\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, GOLD_MINE)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Gather(GatherProblem::Bounce(action)) if action == "mine"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.mine]\nkind = \"gather\"\nrange = \"1\"\nwindup_ms = 1000\ntargeting = \"neutrals\"\nresource = \"silver\"\ntake = 5\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, GOLD_MINE)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Resource, at: Place::Action(action), name } if action == "mine" && name == "silver"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.mine]\nkind = \"gather\"\nrange = \"1\"\nwindup_ms = 1000\ntargeting = \"neutrals\"\nresource = \"gold\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, GOLD_MINE)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::KindField { action, field: ActionDataField::Take } if action == "mine"),
    },
    Flaw {
        file: UNITS,
        edit: Edit::Replace(
            "[units.tower]\n",
            "[units.mine]\nnode = { resource = \"silver\", amount = 1500 }\n\n[units.tower]\n",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Resource, at: Place::UnitType(unit), name } if unit == "mine" && name == "silver"),
    },
    Flaw {
        file: UNITS,
        edit: Edit::Replace(
            "[units.tower]\n",
            "[units.depot]\ndrop_off = { resources = [\"gold\", \"silver\"] }\n\n[units.tower]\n",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Resource, at: Place::UnitType(unit), name } if unit == "depot" && name == "silver"),
    },
    flaw(
        UNITS,
        Edit::Replace(
            "[units.tower]\n",
            "[units.mine]\nnode = { resource = \"gold\", amount = 1500 }\n\n[units.tower]\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Undeclared { capability: Capability::Production, at: Place::UnitType(name) } if name == "mine"),
    ),
    flaw(
        UNITS,
        Edit::Replace(
            r#"slots = { weapon = ["tower_attack"] }"#,
            "slots = { weapon = [\"tower_attack\"] }\nproduction = { queue = 5 }",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Undeclared { capability: Capability::Production, at: Place::UnitType(name) } if name == "tower"),
    ),
    flaw(
        HUSK,
        Edit::Replace(
            "[actions.dread]\n",
            "[actions.dread]\nunit_type = \"melee_creep\"\n",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::KindField { action, field: ActionDataField::UnitType } if action == "dread"),
    ),
    // The release runs actions of kind `cast`, `attack` and `train` alone yet; an action sits in
    // slot kinds of one count of ranks.
    flaw(
        HUSK,
        Edit::Replace("[actions.dread]\n", "[actions.dread]\nkind = \"use\"\n"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::KindNotRun { action, kind: ActionKind::Use } if action == "dread"),
    ),
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[modifiers.warden_blessing]",
            "[actions.taunt]\ntargeting = \"none\"\n\n[modifiers.warden_blessing]",
        ),
        also: &[
            (
                UNITS,
                Edit::Replace(
                    "slots = { weapon = [\"melee_creep_attack\"] }",
                    "slots = { weapon = [\"melee_creep_attack\"], basic = [\"taunt\"] }",
                ),
            ),
            (
                UNITS,
                Edit::Replace(
                    "slots = { weapon = [\"caster_creep_attack\"] }",
                    "slots = { weapon = [\"caster_creep_attack\"], ultimate = [\"taunt\"] }",
                ),
            ),
        ],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::ActionRanks(action) if action == "taunt"),
    },
    // A script adds only to a player resource the mode declares.
    flaw(
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(
            r#"add_resource(killer.owner, "gold", unit.params.gold)"#,
            r#"add_resource(killer.owner, "silver", unit.params.gold)"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Resource, name, .. } if name == "silver"),
    ),
    // A player modifier's filter names a tag the mode has.
    flaw(
        MODE_DATA,
        Edit::Replace(
            "[modifiers.warden_blessing]\n",
            "[modifiers.warden_blessing]\naffects = \"allies:totem\"\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Filter, name: filter, .. } if filter == "allies:totem"),
    ),
    // A relation names two of the mode's teams, a pair once.
    flaw(
        MODE_DATA,
        Edit::Replace(
            "resources = [\"gold\"]\n",
            "resources = [\"gold\"]\n\n[[relations]]\nteams = [\"north\", \"east\"]\nrelation = \"neutral\"\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::UnknownTeam(team)) if team == "east"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            "resources = [\"gold\"]\n",
            "resources = [\"gold\"]\n\n[[relations]]\nteams = [\"north\", \"camps\"]\nrelation = \"neutral\"\n\n[[relations]]\nteams = [\"camps\", \"north\"]\nrelation = \"friendly\"\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::RepeatedRelation(a, b)) if a == "camps" && b == "north"),
    ),
    flaw(
        "heroes/kensho/data/avatar.toml",
        Edit::Replace(r#"targeting = "enemies""#, r#"targeting = "enemies:totem""#),
        "hero-kensho",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Filter, name: filter, .. } if filter == "enemies:totem"),
    ),
    // A second spells package with a spell the first holds.
    Flaw {
        file: MANIFEST,
        edit: Edit::Replace(
            "[dependencies]\n",
            "[dependencies]\nmore-spells = { path = \"../../more\" }\n",
        ),
        also: &[
            (
                "more/manifest.toml",
                Edit::Create(
                    "name = \"more-spells\"\nversion = \"0.1.0\"\napi = \"1.0\"\nlanguage = \"en\"\nkind = \"loadout\"\n",
                ),
            ),
            (
                "more/data/loadout.toml",
                Edit::Create("[actions.haste]\ntargeting = \"none\"\n"),
            ),
        ],
        package: "player-spells",
        refused: |problem| matches!(problem, LoadProblem::Repeated { at: Place::Loadouts, name } if name == "haste"),
    },
];

#[test]
fn every_flaw_of_a_package_fails_its_load_with_its_own_problem() {
    for flaw in &FLAWS {
        let edits = [(flaw.file, flaw.edit)]
            .into_iter()
            .chain(flaw.also.iter().copied());
        let Err(error) = ModePackages::from_package_dir(&edited(edits)) else {
            panic!("{flaw:?} loads");
        };
        let LoadError { package, problem } = &error;
        let named = match package {
            PackageRef::Name(name) => name == flaw.package,
            PackageRef::Dir(dir) => dir.ends_with(flaw.package),
            PackageRef::Fingerprint(_) => false,
        };
        assert!(named && (flaw.refused)(problem), "{flaw:?}: {error:?}");
    }

    // `has_modifier` only names a modifier: Rime's passive's script may ask for the Slow it
    // gives no param of, where an application of it is a flaw.
    let asks = Edit::Replace(
        "m.stacks += 1;",
        r#"if m.carrier.has_modifier("slow") { m.stacks += 1; }"#,
    );
    assert!(ModePackages::from_package_dir(&edited([(STILLNESS, asks)])).is_ok());

    // The three kinds and 253 more are 256, all a byte tells apart; one more fails.
    let kinds = |count: usize| {
        let text = String::from_utf8(moba_files()[Path::new(MODE_DATA)].clone()).unwrap();
        let mut more = String::new();
        for at in 0..count {
            write!(more, ", \"k{at}\"").unwrap();
        }
        let text = text.replacen(
            "damage_kinds = [\"physical\", \"magic\", \"true\"]",
            &format!("damage_kinds = [\"physical\", \"magic\", \"true\"{more}]"),
            1,
        );
        let text: &'static str = Box::leak(text.into_boxed_str());
        ModePackages::from_package_dir(&edited([(MODE_DATA, Edit::Create(text))]))
    };
    assert!(kinds(253).is_ok());
    let error = kinds(254).unwrap_err();
    assert!(
        matches!(*error.problem, LoadProblem::TooMany(Limit::DamageKinds)),
        "{error:?}"
    );

    // The flaws hold each data field and tag property the release plans: `loot` and `noise` as
    // an action's effects, the others as fields.
    let api = CapabilitySet::script_api();
    let mut planned: Vec<_> = api
        .data()
        .iter()
        .filter(|field| field.status == Status::Planned)
        .map(|field| field.name)
        .collect();
    planned.sort_unstable();
    assert_eq!(planned, ["gravity", "loot", "noise", "state_version"]);
    assert!(
        api.tag_properties()
            .iter()
            .all(|held| held.status != Status::Planned)
    );

    // A hero is no mode.
    let husk = ModePackages::from_dir(&moba().join("heroes/husk")).unwrap_err();
    assert!(matches!(*husk.problem, LoadProblem::WrongKind), "{husk}");
}

/// Every ground point of `map` raised to `[x, y, z]`, and the map made spatial: each point of a
/// `pos` and a `points` list to y = 0, and a region from y = 0 to 10, as a spatial region is a
/// box with height; not the bounds, which stay on the ground plane.
fn raised(map: &str) -> String {
    fn raise(value: &mut toml::Value, height: Option<i64>) {
        match value {
            toml::Value::Table(table) => {
                let region = table.contains_key("min") && !table.contains_key("cell");
                for (key, inner) in table.iter_mut() {
                    let height = match key.as_str() {
                        "pos" | "points" => Some(0),
                        "min" if region => Some(0),
                        "max" if region => Some(10),
                        _ => None,
                    };
                    raise(inner, height);
                }
            }
            toml::Value::Array(items) => match height {
                Some(y) if !items[0].is_array() => items.insert(1, toml::Value::Integer(y)),
                _ => {
                    for item in items {
                        raise(item, height);
                    }
                }
            },
            _ => {}
        }
    }
    let mut table = toml::from_str::<toml::Table>(map).unwrap();
    for (key, value) in &mut table {
        if key != "bounds" {
            raise(value, None);
        }
    }
    table.insert(
        "metric".to_owned(),
        toml::Value::String("spatial".to_owned()),
    );
    toml::to_string(&table).unwrap()
}

#[test]
fn a_build_and_a_gather_with_their_nodes_and_drop_offs_load() {
    // A build of a yard, a box with no life pool, so no start life, at a builders' rate table,
    // placed away from enemies; a gather of gold from a mine, which the yard takes back: the load
    // and its books take them all.
    let raise = "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"yard\"\nconstruct = { builders = [\"1\", \"1.5\"] }\ncancel_refund = \"0.75\"\nplacement = { away = [{ filter = \"enemies\", distance = \"4\" }] }\n\n[actions.melee_creep_attack]";
    let edits = [
        (
            MODE_DATA,
            Edit::Replace("[actions.melee_creep_attack]", raise),
        ),
        (MANIFEST, PRODUCTION),
        (UNITS, YARD),
        (
            MODE_DATA,
            Edit::Replace("[actions.melee_creep_attack]", MINE),
        ),
        (UNITS, GOLD_MINE),
        (
            UNITS,
            Edit::Replace(
                "[units.yard]\n",
                "[units.yard]\ndrop_off = { resources = [\"gold\"] }\n",
            ),
        ),
    ];
    let loaded = ModePackages::from_package_dir(&edited(edits));
    assert!(loaded.is_ok(), "{:?}", loaded.err());
}

#[test]
fn a_box_lies_only_on_a_planar_map() {
    // The 3v3 with its towers' bodies boxes: a planar map loads, and the same map made spatial
    // fails, at the first tower, as a box lies on the ground plane.
    let tower = Edit::Replace(
        r#"collision = { radius = "0.9" }"#,
        r#"collision = { box = ["1.8", "1.8"] }"#,
    );
    assert!(ModePackages::from_package_dir(&edited([(UNITS, tower)])).is_ok());
    let map = String::from_utf8(moba_files()[Path::new(MAP)].clone()).unwrap();
    let raised: &'static str = Box::leak(raised(&map).into_boxed_str());
    let spatial = [(MAP, Edit::Create(raised)), (UNITS, tower)];
    let error = ModePackages::from_package_dir(&edited(spatial)).unwrap_err();
    assert!(
        matches!(&*error.problem, LoadProblem::BoxBody { at: Place::UnitType(name), problem: BoxProblem::Spatial } if name == "tower"),
        "{error:?}"
    );
    // The raised map alone loads: the points it raised are its only change.
    let alone = ModePackages::from_package_dir(&edited([(MAP, Edit::Create(raised))]));
    assert!(alone.is_ok(), "{:?}", alone.err());
}
