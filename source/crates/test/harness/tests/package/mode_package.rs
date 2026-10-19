//! Each flaw a package can have fails the load of the MOBA packages with its own problem.

use std::fmt::Write;
use std::num::NonZeroU32;
use std::sync::Arc;

use campfire_actions::{
    ActionField, ActionProblem, CostTarget, EffectData, EffectRule, EffectTo, LaunchLists,
    ListHook, ListedEffect, PipelineField, PlannedEffect, SlotError, SlotKind,
};
use campfire_areas::{AreaDeliveryError, LandEffectData};
use campfire_capabilities::{CapabilityError, Release};
use campfire_codec::Binary;
use campfire_codec::internals::{TomlDocument, TomlNode};
use campfire_combat::{Armor, CombatDataError, HealEffectData};
use campfire_common::{DeclaredName, MapName};
use campfire_declare::{BookPlace, NamesListError, TypePlace};
use campfire_geometry::Metric;
use campfire_math::Num;
use campfire_mode::{ChoiceId, MarkerTag, ModeDataError, ModeError, TimerId};
use campfire_navigation::{MapProblem, NavigationDataError};
use campfire_orders::AiError;
use campfire_package::{
    ContentError, LoadError, LoadProblem, LocaleProblem, ModePackages, PackageName, PackageRef,
    Place, ScriptProblem,
};
use campfire_package_disk::PackageDir;
use campfire_production::ProductionDataError;
use campfire_progression::TrackError;
use campfire_projectiles::{ProjectileDeliveryError, ProjectileError};
use campfire_script::{CtxMisuse, ParamProblem, Status, SyncTo};
use campfire_script::{Filter, internals::Undefined};
use campfire_sim::{Capability, TickRate};
use campfire_stats::{
    AppliedError, ModifierProblem, ModifiersDataError, PurgeEffectData, StatGraphError,
    UnitStartError, Way,
};
use campfire_test_packages::{Edit, PackageTree, TestMode};
use campfire_units::{HeightGrid, Layer, PoolId, StatId, TypesError, UnitTypeError};
use campfire_values::{
    ActionId, DamageKind, EngineEnum, MessageId, ModifierId, Named, Number, Param, ResourceId,
    Scalar, Stat, Tag, Team, TimeTooLarge, TrackId, UnitType,
};
use campfire_vision::VisionDataError;

use crate::package::moba::{edited, load, moba, two_lanes};
use crate::package::module_rules::{entry, rule, rule_at, rule_of, unit_type};

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
const MAP: &str = "modes/3v3/map/two_lanes/map.toml";
const HUSK: &str = "heroes/husk/data/avatar.toml";
const HUSK_MANIFEST: &str = "heroes/husk/manifest.toml";
const HUSK_TEXT: &str = "heroes/husk/locale/en.ftl";
const MODE_TEXT: &str = "modes/3v3/locale/en.ftl";
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
    let error = AppliedError::Param {
        param: name("slow"),
        way: way.cloned(),
        problem: kind,
    };
    rule_at(problem, &entry("modifiers", "slow"), &error)
}

/// Whether `problem` is the modifier `modifier` held as the passive of `first` and of the
/// action `second` of one package.
fn shared_passive(problem: &LoadProblem, modifier: &str, first: BookPlace, second: &str) -> bool {
    let error = AppliedError::SharedPassive {
        modifier: name(modifier),
        first,
    };
    rule_at(problem, &entry("actions", second), &error)
}

/// The place of the action `id`, as a module's rule names it.
fn action_place(id: &str) -> BookPlace {
    BookPlace::Entry {
        table: "actions",
        name: name(id),
    }
}

/// Whether `problem` is live stat changes that read each other in a loop through `stats`.
fn stat_loop<const N: usize>(problem: &LoadProblem, stats: [&str; N]) -> bool {
    let looped = stats.map(|stat| Stat::named(stat).unwrap()).to_vec();
    rule_at(
        problem,
        &Place::Section("stats"),
        &StatGraphError::Loop(looped),
    )
}

fn name(name: &str) -> DeclaredName {
    DeclaredName::new(name).unwrap()
}

/// Whether `problem` is a script that does not compile, as it reads the variable `name`, which
/// nothing defines before it.
fn undefined(problem: &LoadProblem, name: &str) -> bool {
    matches!(problem, LoadProblem::Script { problem: ScriptProblem::Compile(error), .. } if error.undefined() == Some(Undefined::Variable(name)))
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
    let error = load(&edited([(UNITS, edit)])).unwrap_err();
    assert_eq!(
        error.package,
        PackageRef::Name(PackageName::new(MODE).unwrap())
    );
    let at_caster = |problem: &LoadProblem| {
        rule_at(
            problem,
            &unit_type("caster_creep_bolt"),
            &ProjectileError::NotFaster,
        )
    };
    assert!(at_caster(&error.problem), "{error:?}");
    // Along a line, the same speed loads: it chases no one.
    let edit = Edit::Set("units.grasping_wraps.projectile.speed", r#""5""#);
    assert!(load(&edited([(HUSK, edit)])).is_ok());
}

#[test]
fn a_weapon_or_an_action_at_a_unit_or_a_point_may_fly_a_curve_and_one_along_a_direction_not() {
    // The tower's weapon flies a curve, 5 m/s, slower than the cap: a curve chases no one, and
    // its end follows its target.
    let curve = r#"{ first = ["0.3", "4"], second = ["0.7", "4"], follow = "2", life_ms = 3000 }"#;
    let shell = format!(r#"{{ speed = "5", curve = {curve} }}"#);
    let edit = Edit::Set("units.tower_bolt.projectile", &shell);
    assert!(load(&edited([(UNITS, edit)])).is_ok());
    // One that hits no unit but its target meets that by its width, so it may be wide.
    let aimed = format!(r#"{{ speed = "5", width = "2", hits = "none", curve = {curve} }}"#);
    let edit = Edit::Set("units.tower_bolt.projectile", &aimed);
    assert!(load(&edited([(UNITS, edit)])).is_ok());
    // The husk's wraps aim along a direction, which a curve does not fly.
    let wraps = format!(r#"{{ speed = "20", width = "1.6", curve = {curve} }}"#);
    let edit = Edit::Set("units.grasping_wraps.projectile", &wraps);
    let error = load(&edited([(HUSK, edit)])).unwrap_err();
    let along = |problem: &LoadProblem| {
        rule_at(
            problem,
            &entry("actions", "grasping_wraps"),
            &ProjectileDeliveryError::Curve,
        )
    };
    assert!(along(&error.problem), "{error:?}");
    // A curve takes nothing of a line's or a homing flight's, and its points lie on the way.
    for (projectile, message) in [
        (format!(r#"{{ speed = "5", homing = true, curve = {curve} }}"#), "a curve takes no"),
        (format!(r#"{{ speed = "5", stop_on_hit = true, curve = {curve} }}"#), "a curve takes no"),
        (
            r#"{ speed = "5", curve = { first = ["1.5", "4"], second = ["0.7", "4"] } }"#.to_owned(),
            "a curve's point",
        ),
        (
            r#"{ speed = "5", curve = { first = ["0.3", "4"], second = ["0.7", "4"], life_ms = 0 } }"#
                .to_owned(),
            "a curve's `life_ms`",
        ),
    ] {
        let edit = Edit::Set("units.tower_bolt.projectile", &projectile);
        let error = load(&edited([(UNITS, edit)])).unwrap_err();
        let file = "data/units.toml";
        assert!(read_fails(&error.problem, file, message), "{projectile}: {error:?}");
    }
}

#[test]
fn a_weapons_splash_needs_a_projectile_its_scatter_a_curve_its_speed_none_and_its_pitch_a_bound() {
    let splash = (
        "actions.tower_attack.splash",
        r#"{ radius = "2", affects = "enemies", spares_source = false }"#,
    );
    let scatter = (
        "actions.tower_attack.scatter",
        r#"{ radius = "3", tags = { creep = "2" } }"#,
    );
    let pitch = ("actions.tower_attack.pitch_deg", r#"["-10", "45"]"#);
    let curve = (
        "units.tower_bolt.projectile",
        r#"{ speed = "20", curve = { first = ["0.3", "4"], second = ["0.7", "4"] } }"#,
    );
    let set = |edits: &[(&'static str, &'static str)]| {
        let mut files: Vec<(&str, Edit<'static>)> = Vec::new();
        for &(path, value) in edits {
            let file = if path.starts_with("units.") {
                UNITS
            } else {
                MODE_DATA
            };
            files.push((file, Edit::Set(path, value)));
        }
        load(&edited(files))
    };
    // A homing bolt splashes and bounds its pitch; a curve scatters too.
    assert!(set(&[splash, pitch]).is_ok());
    assert!(set(&[splash, scatter, pitch, curve]).is_ok());
    // A homing bolt does not scatter, and a weapon of no projectile does not splash.
    let error = set(&[scatter]).unwrap_err();
    let scatters = |problem: &LoadProblem| {
        rule_at(
            problem,
            &entry("actions", "tower_attack"),
            &CombatDataError::Scatter,
        )
    };
    assert!(scatters(&error.problem), "{error:?}");
    // A melee weapon may be slow; a homing bolt flies at its own speed.
    assert!(set(&[("actions.wolf_attack.speed", r#""40""#)]).is_ok());
    let error = set(&[("actions.tower_attack.speed", r#""40""#)]).unwrap_err();
    let speeds = |problem: &LoadProblem| {
        rule_at(
            problem,
            &entry("actions", "tower_attack"),
            &CombatDataError::Speed,
        )
    };
    assert!(speeds(&error.problem), "{error:?}");
    let melee = [("actions.wolf_attack.splash", splash.1)];
    let error = set(&melee).unwrap_err();
    let splashes = |problem: &LoadProblem| {
        rule_at(
            problem,
            &entry("actions", "wolf_attack"),
            &CombatDataError::Splash,
        )
    };
    assert!(splashes(&error.problem), "{error:?}");
    // Each reads only within its bounds.
    let file = "data/mode.toml";
    for (edit, message) in [
        (
            (
                "actions.tower_attack.splash",
                r#"{ radius = "0", affects = "enemies" }"#,
            ),
            "a splash's radius",
        ),
        (
            ("actions.tower_attack.scatter", r#"{ radius = "-1" }"#),
            "a scatter's radius",
        ),
        (
            ("actions.tower_attack.pitch_deg", r#"["45", "-10"]"#),
            "a pitch's bound",
        ),
        (
            ("actions.tower_attack.pitch_deg", r#"["90", "90"]"#),
            "a pitch's bound",
        ),
        (
            ("actions.wolf_attack.speed", r#""0""#),
            "a speed is a positive number",
        ),
    ] {
        let error = set(&[edit]).unwrap_err();
        assert!(
            read_fails(&error.problem, file, message),
            "{edit:?}: {error:?}"
        );
    }
}

#[test]
fn a_mode_state_field_is_sent_to_no_client_unless_it_says() {
    // The 3v3's `phase` is sent to all; without `sync`, or with `sync = "none"`, to none.
    let phase = |edit: Edit<'_>| {
        let packages = load(&edited([(MODE_DATA, edit)])).unwrap();
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
    let packages = load(&edited([(RIME, to_source)])).unwrap();
    let rime = packages
        .dependencies()
        .iter()
        .find(|dependent| dependent.package.header.name == "hero-rime")
        .unwrap();
    let heal = EffectData {
        does: Arc::new(HealEffectData {
            amount: Number::Value(Scalar::Int(1)),
        }),
        to: EffectTo::Source,
    };
    assert_eq!(
        rime.content.actions()["fan_of_frost"].on_hit.last(),
        Some(&heal)
    );
    // A purge names a tag the match declares, which Rime's slow grants.
    let purge = Edit::Replace(SLOWS, r#"{ purge = { tag = "slowed" } },"#);
    let packages = load(&edited([(RIME, purge)])).unwrap();
    let rime = packages
        .dependencies()
        .iter()
        .find(|dependent| dependent.package.header.name == "hero-rime")
        .unwrap();
    let purge: Arc<dyn ListedEffect> = Arc::new(PurgeEffectData {
        tag: DeclaredName::new("slowed").unwrap(),
    });
    assert_eq!(
        rime.content.actions()["fan_of_frost"]
            .on_hit
            .last()
            .map(|effect| &effect.does),
        Some(&purge)
    );
    // `to` names the source alone.
    let to_target = Edit::Replace(SLOWS, r#"{ heal = { amount = 1 }, to = "target" },"#);
    let error = load(&edited([(RIME, to_target)])).unwrap_err();
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
    let packages = load(&edited([(MODE_DATA, edit)])).unwrap();
    let weapon = &packages.content().actions()["melee_creep_attack"];
    assert_eq!((weapon.on_hit.len(), weapon.params.len()), (1, 1));
    let on_end = biting("on_end");
    let edit = Edit::Replace(MELEE_END, &on_end);
    let error = load(&edited([(MODE_DATA, edit)])).unwrap_err();
    let refused = |problem: &LoadProblem| {
        rule_at(
            problem,
            &entry("actions", "melee_creep_attack"),
            &ActionProblem::KindField(PipelineField::OnEnd),
        )
    };
    assert!(refused(&error.problem), "{error:?}");
    // Cinder's bolt launches an eruption, an area type of her own package, where it hits.
    let edit = Edit::Set(
        "actions.attack.on_hit",
        r#"[{ land = { area = "eruption" } }]"#,
    );
    let packages = load(&edited([(CINDER, edit)])).unwrap();
    let cinder = packages
        .dependencies()
        .iter()
        .find(|dependent| dependent.package.header.name == "hero-cinder")
        .unwrap();
    let launch: Arc<dyn ListedEffect> = Arc::new(LandEffectData {
        area: DeclaredName::new("eruption").unwrap(),
        lists: LaunchLists::default(),
    });
    let on_hit = &cinder.content.actions()["attack"].on_hit;
    assert_eq!(
        on_hit.iter().map(|effect| &effect.does).collect::<Vec<_>>(),
        [&launch]
    );
}

/// The 3v3 as its packages hold it.
fn three_v_three() -> ModePackages {
    PackageDir::new(moba().join(MODE_DIR))
        .mode_packages(Some(&two_lanes()))
        .unwrap()
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
    let packages = load(&edited(edits)).unwrap();
    assert!(packages.content().units.contains_key("grasping_wraps"));
}

#[test]
fn a_script_names_a_spawn_tag_no_marker_of_the_map_holds() {
    // `camps`, a spawn tag of the mode's that no marker of the map holds, is a tag its script
    // may name, as a mode names the markers it spawns at whatever map it plays.
    let edits = [
        (
            "modes/3v3/scripts/mode.rhai",
            Edit::Replace(r#"ctx.map.markers("camp")"#, r#"ctx.map.markers("camps")"#),
        ),
        (
            MODE_DATA,
            Edit::Replace(
                r#"spawn_tags = ["camp", "spawn"]"#,
                r#"spawn_tags = ["camp", "camps", "spawn"]"#,
            ),
        ),
    ];
    assert!(load(&edited(edits)).is_ok());
}

#[test]
fn an_anonymous_function_and_a_function_pointer_load() {
    // An anonymous function captures nothing, and a pointer passes `ctx` to none.
    for line in [
        "    let near = |unit| unit.pos;\n",
        "    let hook = Fn(\"on_resolve\");\n",
    ] {
        let at = r#"    for unit in ctx.find(caster, caster.pos, ctx.params.radius, "enemies") {"#;
        let edit = Edit::Replace(at, &format!("{line}{at}"));
        assert!(load(&edited([(LASH_OUT, edit)])).is_ok(), "{line}");
    }
}

#[test]
fn a_mode_script_spawns_its_own_unit_types_and_avatars_by_name() {
    // The mode's tower, and Husk by its package's name, are in the mode's scope; the team is one
    // of the manifest's.
    let spawns = r#"    ctx.spawn_unit("tower", "north", unit.pos);
    ctx.spawn_unit("hero-husk", "camps", unit.pos);
    if unit.has_tag("inhibitor") {
"#;
    let edit = Edit::Replace("    if unit.has_tag(\"inhibitor\") {\n", spawns);
    assert!(load(&edited([(MODE_SCRIPT, edit)])).is_ok());
}

#[test]
fn the_engines_tags_and_the_modes_fill_the_tags_a_match_holds() {
    // The 3v3 names 25 tags: the 10 of its `[tags]`, 6 of its heroes' classes, 8 more of its unit
    // types', `ward` among them, and `slowed`, which modifiers grant. The engine has 8, so 223
    // layers, each a tag, fill the 256 a match holds, and 224 are past it.
    let tags = |packages: &ModePackages| packages.tag_names().len();
    let packages = load(&edited([])).unwrap();
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
    let [(path, layers)] = <[_; 1]>::try_from(layers(223)).unwrap();
    let edit = Edit::Set(&path, &layers);
    let packages = load(&edited([(MODE_DATA, edit)])).unwrap();
    assert_eq!(tags(&packages), 248);
    // The mode's `[tags]` may give an engine tag properties, and that names no tag of its own.
    let edit = Edit::Replace(
        "[tags.stunned]",
        "[tags.projectile]\nhidden = true\n\n[tags.stunned]",
    );
    let packages = load(&edited([(MODE_DATA, edit)])).unwrap();
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
    refused: fn(&LoadProblem) -> bool,
}

#[test]
fn a_mode_loads_up_to_each_limit_and_fails_one_past_it() {
    let cases = [
        // The 3v3's 25 tags and the engine's 8 leave 223 of the 256 a match holds for layers.
        LimitCase {
            more: layers,
            allowed: 223,
            refused: |problem| rule(problem, &TypesError::TooManyTags),
        },
        // `level` and 31 more fill the 32 tracks a unit holds.
        LimitCase {
            more: tracks,
            allowed: 31,
            refused: |problem| rule(problem, &TrackError::TooMany),
        },
    ];
    for case in cases {
        let load = |count| {
            let sets = (case.more)(count);
            let edits = sets
                .iter()
                .map(|(path, value)| (MODE_DATA, Edit::Set(path, value)));
            load(&edited(edits))
        };
        assert!(load(case.allowed).is_ok(), "{case:?}");
        let error = load(case.allowed + 1).unwrap_err();
        assert_eq!(
            error.package,
            PackageRef::Name(PackageName::new(MODE).unwrap())
        );
        assert!((case.refused)(&error.problem), "{case:?}: {error:?}");
    }
}

/// Flaws of the manifest, the data's sections, the tracks, the slot kinds' levels, the call
/// budget, and an action's delivery, spawn and move: each to a copy of the packages, and the
/// problem it fails the load with.
static DATA_FLAWS: &[Flaw] = &[
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
    // A newer package's manifest may hold a field this release does not know: its version is
    // read first, so it fails as `OtherApi`, the mode's and a dependency's alike.
    Flaw {
        file: MANIFEST,
        edit: Edit::Set("api", r#""1.1""#),
        also: &[(MANIFEST, Edit::Set("future", "true"))],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::OtherApi(api) if api.to_string() == "1.1"),
    },
    Flaw {
        file: "heroes/husk/manifest.toml",
        edit: Edit::Set("api", r#""2.0""#),
        also: &[("heroes/husk/manifest.toml", Edit::Set("future", "true"))],
        package: "hero-husk",
        refused: |problem| matches!(problem, LoadProblem::OtherApi(api) if api.to_string() == "2.0"),
    },
    flaw(
        MANIFEST,
        Edit::Replace(r#"["combat","#, r#"["mode", "combat","#),
        MODE_DIR,
        |problem| {
            matches!(
                problem,
                LoadProblem::Capabilities(CapabilityError::DeclaresMode)
            )
        },
    ),
    flaw(
        MANIFEST,
        Edit::Replace(r#"["combat","#, r#"["combat", "combat","#),
        MODE_DIR,
        |problem| {
            matches!(
                problem,
                LoadProblem::Capabilities(CapabilityError::Repeated(Capability::Combat))
            )
        },
    ),
    flaw(
        MANIFEST,
        Edit::Replace(r#", "navigation""#, ""),
        MODE_DIR,
        |problem| {
            matches!(
                problem,
                LoadProblem::Capabilities(CapabilityError::Needs {
                    capability: Capability::Orders,
                    needs: Capability::Navigation,
                })
            )
        },
    ),
    // A section of the mode's data or of its map is its capability's, declared or refused: the
    // map's vision grid without `vision`, the mode's `[navigation]` without `navigation`,
    // `[supply]` without `production`.
    flaw(
        MANIFEST,
        Edit::Replace(r#", "vision", "progression""#, r#", "progression""#),
        MODE,
        |problem| {
            matches!(
                problem,
                LoadProblem::Undeclared {
                    capability: Capability::Vision,
                    at: Place::MapSection("grid")
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
                    at: Place::Section("navigation")
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
                    at: Place::Section("supply")
                }
            )
        },
    ),
    // Tracks and a death's experience are progression's: tracks positive and ascending, at most
    // one the `level` track, and each a unit type lists one the mode declares; the experience
    // section the first the load finds.
    flaw(
        MANIFEST,
        Edit::Replace(r#", "progression", "items"]"#, r#", "items"]"#),
        MODE,
        |problem| {
            matches!(
                problem,
                LoadProblem::Undeclared {
                    capability: Capability::Progression,
                    at: Place::Section("experience")
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
        |problem| rule(problem, &TrackError::LevelTracks),
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
        |problem| {
            rule(
                problem,
                &TrackError::RankLevels(DeclaredName::new("basic").unwrap()),
            )
        },
    ),
    flaw(
        HUSK,
        Edit::Replace(r#"tracks = ["level"]"#, r#"tracks = ["levels"]"#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: TrackId::NAMES, name, .. } if name == "levels"),
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
        |problem| matches!(problem, LoadProblem::Unknown { of: ActionId::NAMES, at: Place::Avatar(_), name } if name == "tomb_binds"),
    ),
    flaw(
        HUSK,
        Edit::Set(
            "actions.grasping_wraps.cooldown_ms",
            "[16000, 14000, 12000, 10000]",
        ),
        "hero-husk",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "grasping_wraps"),
                &ActionProblem::RankCount(5),
            )
        },
    ),
    flaw(
        HUSK,
        Edit::Replace(
            "[combat]\n",
            "orders = { ai = \"scripts/lash_out.rhai\", think_ms = 250 }\n[combat]\n",
        ),
        "hero-husk",
        |problem| rule(problem, &AiError::AvatarOrders),
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
        |problem| {
            rule_at(
                problem,
                &entry("actions", "grasping_wraps"),
                &ProjectileDeliveryError::Homing,
            )
        },
    ),
    flaw(
        HUSK,
        Edit::Replace(
            r#"delivery = { projectile = "grasping_wraps" }"#,
            r#"delivery = { projectile = "wraps" }"#,
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "grasping_wraps" && name == "wraps"),
    ),
    flaw(
        HUSK,
        Edit::Set("actions.grasping_wraps.targeting", r#""none""#),
        "hero-husk",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "grasping_wraps"),
                &ProjectileDeliveryError::NoAim,
            )
        },
    ),
    flaw(
        HUSK,
        Edit::Replace(
            "[combat]\n",
            "projectile = { speed = \"20\" }\n\n[combat]\n",
        ),
        "hero-husk",
        |problem| {
            rule_of(problem, |at, error: &UnitTypeError| {
                matches!(at, Place::Avatar(_)) && *error == UnitTypeError::NotDelivery
            })
        },
    ),
    flaw(
        UNITS,
        Edit::Replace(
            "[units.tower_bolt]\n",
            "[units.tower_bolt]\npools = [\"health\"]\n",
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &unit_type("tower_bolt"),
                &UnitTypeError::NotDelivery,
            )
        },
    ),
    flaw(
        UNITS,
        Edit::Remove("units.tower_bolt.projectile.homing"),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("actions", "tower_attack"),
                &CombatDataError::Delivery,
            )
        },
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            r#"delivery = { projectile = "tower_bolt" }"#,
            r#"delivery = { projectile = "tower" }"#,
        ),
        MODE,
        |problem| {
            let wrong = ProjectileDeliveryError::NotProjectile(DeclaredName::new("tower").unwrap());
            rule_at(problem, &entry("actions", "tower_attack"), &wrong)
        },
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
        |problem| {
            let wrong = AreaDeliveryError::NotArea(DeclaredName::new("fire_lance").unwrap());
            rule_at(problem, &entry("actions", "eruption"), &wrong)
        },
    ),
    flaw(
        CINDER,
        Edit::Set("actions.eruption.targeting", r#""direction""#),
        "hero-cinder",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "eruption"),
                &AreaDeliveryError::Direction,
            )
        },
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
        |problem| matches!(problem, LoadProblem::Unknown { of: Filter::NAMES, at: Place::UnitType(name), .. } if name == "eruption"),
    ),
    flaw(
        CINDER,
        Edit::Replace(
            "[units.eruption]\n",
            "[units.eruption]\nprojectile = { speed = \"20\" }\n",
        ),
        "hero-cinder",
        |problem| rule_at(problem, &unit_type("eruption"), &UnitTypeError::NotDelivery),
    ),
    flaw(
        VEIL,
        Edit::Replace(r#"self = "smoke_ring_cover""#, r#"self = "smoke_cover""#),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::Unknown { of: ModifierId::NAMES, at: Place::UnitType(unit_type), name } if unit_type == "smoke_ring" && name == "smoke_cover"),
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
        |problem| {
            rule(
                problem,
                &ModeError::UnknownUnitType(DeclaredName::new("tower_bolt").unwrap()),
            )
        },
    ),
    // Snow Owl hits nothing: a width, a stop on hit, a hit once a cast or homing would use hits,
    // and so would an `on_hit` list or hook of its action.
    flaw(
        RIME,
        Edit::Set("units.snow_owl.projectile.width", r#""0.5""#),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &unit_type("snow_owl"),
                &ProjectileError::HitsNothing,
            )
        },
    ),
    flaw(
        RIME,
        Edit::Set("units.snow_owl.projectile.stop_on_hit", "true"),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &unit_type("snow_owl"),
                &ProjectileError::HitsNothing,
            )
        },
    ),
    flaw(
        RIME,
        Edit::Set("units.snow_owl.projectile.once_per_cast", "true"),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &unit_type("snow_owl"),
                &ProjectileError::HitsNothing,
            )
        },
    ),
    flaw(
        RIME,
        Edit::Set("units.snow_owl.projectile.homing", "true"),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &unit_type("snow_owl"),
                &ProjectileError::HitsNothing,
            )
        },
    ),
    flaw(
        RIME,
        Edit::Set(
            "actions.snow_owl.on_hit",
            r#"[{ damage = { amount = 1, kind = "true" } }]"#,
        ),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "snow_owl"),
                &ProjectileDeliveryError::NoHit,
            )
        },
    ),
    flaw(
        SNOW_OWL,
        Edit::Replace(
            "fn on_end(ctx, caster, hit) {",
            "fn on_hit(ctx, caster, target, hit) {}\n\nfn on_end(ctx, caster, hit) {",
        ),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "snow_owl"),
                &ProjectileDeliveryError::NoHit,
            )
        },
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
            r#"[{ land = { area = "fire_lance" } }]"#,
        ),
        "hero-cinder",
        |problem| {
            let wrong = AreaDeliveryError::NotArea(DeclaredName::new("fire_lance").unwrap());
            rule_at(problem, &entry("actions", "attack"), &wrong)
        },
    ),
    flaw(
        CINDER,
        Edit::Set("actions.attack.on_hit", r#"[{ land = { area = "pyre" } }]"#),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "attack" && name == "pyre"),
    ),
    flaw(
        CINDER,
        Edit::Set(
            "actions.attack.on_hit",
            r#"[{ land = { area = "eruption", on_end = [{ heal = { amount = 1 } }] } }]"#,
        ),
        "hero-cinder",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "attack"),
                &ActionProblem::Effect {
                    list: ListHook::OnEnd,
                    rule: EffectRule::NoUnit,
                },
            )
        },
    ),
    // An action spawns a unit type of its own package that stands, an avatar's summon among
    // them, for a whole number of milliseconds: Rime's arrow, a projectile, is none. A summon of
    // Rime's names only what the mode declares, and one of the spells' loadout slots its actions
    // only in kinds of the loadout's ranks. Loot is planned.
    flaw(
        RIME,
        Edit::Replace(SLOWS, r#"{ spawn = { unit_type = "frost_arrow" } },"#),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, name, .. } if name == "frost_arrow"),
    ),
    flaw(
        RIME,
        Edit::Set("units.wisp.stats.nonsense.base", "1"),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, name, .. } if name == "nonsense"),
    ),
    flaw(
        "spells/data/loadout.toml",
        Edit::Set("units.totem.slots.basic", r#"["blink"]"#),
        "player-spells",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "blink"),
                &ActionProblem::SlottedRanks,
            )
        },
    ),
    flaw(
        RIME,
        Edit::Replace(SLOWS, r#"{ loot = { table = "chest", level = 1 } },"#),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "fan_of_frost"),
                &ActionProblem::Effect {
                    list: ListHook::OnHit,
                    rule: EffectRule::Planned(PlannedEffect::Loot),
                },
            )
        },
    ),
    flaw(
        MODE_DATA,
        Edit::Set(
            "actions.tower_attack.on_hit",
            r#"[{ spawn = { unit_type = "caster_creep_bolt" } }]"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, name, .. } if name == "caster_creep_bolt"),
    ),
    flaw(
        MODE_DATA,
        Edit::Set(
            "actions.tower_attack.on_hit",
            r#"[{ spawn = { unit_type = "melee_creep", duration_ms = "1.5" } }]"#,
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("actions", "tower_attack"),
                &ActionProblem::Effect {
                    list: ListHook::OnHit,
                    rule: EffectRule::Duration,
                },
            )
        },
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
        |problem| {
            rule_at(
                problem,
                &entry("actions", "fan_of_frost"),
                &ActionProblem::Effect {
                    list: ListHook::OnHit,
                    rule: EffectRule::Duration,
                },
            )
        },
    ),
    flaw(
        CINDER,
        Edit::Set(
            "actions.attack.on_hit",
            r#"[{ land = { area = "eruption", on_end = [{ move = { to = "reached", speed = 5 }, to = "source" }] } }]"#,
        ),
        "hero-cinder",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "attack"),
                &ActionProblem::Effect {
                    list: ListHook::OnEnd,
                    rule: EffectRule::NoUnit,
                },
            )
        },
    ),
    flaw(
        RIME,
        Edit::Replace(SLOWS, r#"{ purge = { tag = "frozen" } },"#),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Unknown { of: Tag::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "fan_of_frost" && name == "frozen"),
    ),
    flaw(
        RIME,
        Edit::Replace(SLOWS, r#"{ purge = { tag = "avatar" } },"#),
        "hero-rime",
        |problem| {
            let purged = EffectRule::EngineTag(DeclaredName::new("avatar").unwrap());
            let effect = ActionProblem::Effect {
                list: ListHook::OnHit,
                rule: purged,
            };
            rule_at(problem, &entry("actions", "fan_of_frost"), &effect)
        },
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
            let broken = EffectRule::NoUnit;
            let effect = ActionProblem::Effect {
                list: ListHook::OnEnd,
                rule: broken,
            };
            rule(problem, &effect)
        },
    ),
    flaw(
        RIME,
        Edit::Replace("on_hit = [", "on_resolve = ["),
        "hero-rime",
        |problem| {
            let broken = EffectRule::NoUnit;
            let effect = ActionProblem::Effect {
                list: ListHook::OnResolve,
                rule: broken,
            };
            rule(problem, &effect)
        },
    ),
    flaw(
        RIME,
        Edit::Remove("actions.fan_of_frost.delivery"),
        "hero-rime",
        |problem| {
            let broken = EffectRule::NoDelivery;
            let effect = ActionProblem::Effect {
                list: ListHook::OnHit,
                rule: broken,
            };
            rule(problem, &effect)
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
            rule_of(problem, |_, error: &ActionProblem| {
                matches!(
                    error,
                    ActionProblem::Effect {
                        rule: EffectRule::Negative,
                        ..
                    }
                )
            })
        },
    ),
];

/// Flaws of an effect's amount, a handle, a state field and a script's compile.
static SCRIPT_FLAWS: &[Flaw] = &[
    // An amount of i64::MAX, 2⁶³ − 1, is past what a sim number holds, whose most is
    // (2⁶³ − 1) / 2²⁴.
    flaw(
        RIME,
        Edit::Set(
            "actions.fan_of_frost.on_hit",
            r#"[{ damage = { amount = 9223372036854775807, kind = "true" } }]"#,
        ),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "fan_of_frost"),
                &ActionProblem::Effect {
                    list: ListHook::OnHit,
                    rule: EffectRule::Overflow,
                },
            )
        },
    ),
    flaw(
        RIME,
        Edit::Set("actions.fan_of_frost.params.slow_ms", r#""2000.5""#),
        "hero-rime",
        |problem| {
            rule_of(problem, |_, error: &ActionProblem| {
                matches!(
                    error,
                    ActionProblem::Effect {
                        rule: EffectRule::Duration,
                        ..
                    }
                )
            })
        },
    ),
    flaw(
        RIME,
        Edit::Replace(r#"kind = "physical" } }"#, r#"kind = "frost" } }"#),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Unknown { of: DamageKind::NAMES, name, .. } if name == "frost"),
    ),
    flaw(
        RIME,
        Edit::Replace(
            r#"id = "slow", duration_ms"#,
            r#"id = "frozen", duration_ms"#,
        ),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Unknown { of: ModifierId::NAMES, name, .. } if name == "frozen"),
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
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "recruit"),
                &ProductionDataError::Trained,
            )
        },
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
    // A handle's param that no unit type or map marker holds.
    flaw(
        CREEP_AI,
        Edit::Replace(
            "ctx.nearest_visible(unit, unit.params.aggro_range, \"enemies:creep\")",
            "ctx.nearest_visible(unit, unit.params.aggro_rang, \"enemies:creep\")",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: Param::NAMES, at: Place::Script(path), name } if path.as_str() == "scripts/creep_ai.rhai" && name == "aggro_rang"),
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
            r#"ctx.damage(unit, "magic", ctx.params.damage);"#,
            r#"ctx.damage(unti, "magic", ctx.params.damage);"#,
        ),
        "hero-husk",
        |problem| undefined(problem, "unti"),
    ),
    flaw(
        CREEP_AI,
        Edit::Replace(
            "let target = unit.attack_target;",
            "let target = unti.attack_target;",
        ),
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
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::Compile(error), .. } if error.undefined() == Some(Undefined::Module("Path"))),
    ),
    // A literal compared with a name the API returns is a name of its kind, as the mode's damage
    // kinds `true` and `physical` are; an action's name compares through `is_action`, in its
    // package's scope; a `switch` on a name has cases no check reads.
    flaw(
        MODE_SCRIPT,
        Edit::Replace(r#"d.kind == "true""#, r#"d.kind == "pure""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: DamageKind::NAMES, at: Place::Script(_), name } if name == "pure"),
    ),
    flaw(
        MODE_SCRIPT,
        Edit::Replace(r#"d.kind == "true""#, r#"d.action == "true""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::ScopedCompare(member), .. } if member == "action"),
    ),
    flaw(
        MODE_SCRIPT,
        Edit::Replace(
            r#"if d.kind == "true" {"#,
            r#"switch d.kind { "true" => {} }
    if d.kind == "true" {"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::NameSwitch(member), .. } if member == "kind"),
    ),
    // No script holds statements outside its functions, and none tries to catch a refusal.
    flaw(
        LASH_OUT,
        Edit::Replace(
            "fn on_resolve(ctx, caster, target) {",
            "let radius = 3;\nfn on_resolve(ctx, caster, target) {",
        ),
        "hero-husk",
        |problem| {
            matches!(
                problem,
                LoadProblem::Script {
                    problem: ScriptProblem::TopLevel,
                    ..
                }
            )
        },
    ),
    flaw(
        LASH_OUT,
        Edit::Replace(
            r#"        ctx.damage(unit, "magic", ctx.params.damage);"#,
            r#"        try { ctx.damage(unit, "magic", ctx.params.damage); } catch { }"#,
        ),
        "hero-husk",
        |problem| {
            matches!(
                problem,
                LoadProblem::Script {
                    problem: ScriptProblem::Compile(_),
                    ..
                }
            )
        },
    ),
    // With no closures, a function that captures a variable fails to compile: it names a
    // variable its scope lacks.
    flaw(
        LASH_OUT,
        Edit::Replace(
            r#"    for unit in ctx.find(caster, caster.pos, ctx.params.radius, "enemies") {"#,
            r#"    let radius = ctx.params.radius;
    let near = |unit| unit.pos.within(caster.pos, radius);
    for unit in ctx.find(caster, caster.pos, radius, "enemies") {"#,
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::Compile(error), .. } if error.undefined() == Some(Undefined::Variable("caster"))),
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
    // A call or a method given a count of arguments no form of it takes, as `ctx.damage` takes
    // three and `distance_to` one.
    flaw(
        LASH_OUT,
        Edit::Replace(
            "ctx.damage(unit, \"magic\", ctx.params.damage);",
            "ctx.damage(unit);",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::Arity { name, args: 1 }, .. } if name == "ctx.damage"),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace(
            "    for unit in",
            "    let far = caster.pos.distance_to();\n    for unit in",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::Arity { name, args: 0 }, .. } if name == ".distance_to"),
    ),
];

/// Flaws of a weapon, a place on the map, an AI, the visibility, the names the release plans,
/// a cooldown cut, a hold, a stat change and the loops of stats.
static ACTION_FLAWS: &[Flaw] = &[
    // A weapon has a rate, a damage and a damage kind the mode declares, aims at a unit within
    // meters, and has no field only a cast runs; no other kind has a weapon's fields.
    flaw(
        MODE_DATA,
        Edit::Replace(
            "damage_kind = \"physical\"\ndelivery = { projectile = \"tower_bolt\" }",
            "damage_kind = \"fire\"\ndelivery = { projectile = \"tower_bolt\" }",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: DamageKind::NAMES, at: Place::Entry { table: "actions", name: action }, name: kind } if action == "tower_attack" && kind == "fire"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("rate = \"attack_speed\"\n", "rate = \"attack_sped\"\n"),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "melee_creep_attack" && name == "attack_sped"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("rate = \"attack_speed\"\n", ""),
        MODE,
        |problem| read_fails(problem, "data/mode.toml", "a weapon's rate is a stat"),
    ),
    flaw(
        MODE_DATA,
        Edit::Set("actions.tower_attack.range", r#""global""#),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("actions", "tower_attack"),
                &ActionProblem::Global,
            )
        },
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            "[actions.tower_attack]\nkind = \"attack\"\ntargeting = \"enemies\"",
            "[actions.tower_attack]\nkind = \"attack\"\ntargeting = \"point\"",
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("actions", "tower_attack"),
                &CombatDataError::AttackAims,
            )
        },
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            "[actions.wolf_attack]\n",
            "[actions.wolf_attack]\ncooldown_ms = 1000\n",
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("actions", "wolf_attack"),
                &ActionProblem::KindField(PipelineField::CooldownMs),
            )
        },
    ),
    flaw(
        HUSK,
        Edit::Replace(
            "[actions.dread]\n",
            "[actions.dread]\nrate = \"attack_speed\"\n",
        ),
        "hero-husk",
        |problem| read_fails(problem, "data/avatar.toml", "unknown field `rate`"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            "damage_kinds = [\"physical\", \"magic\", \"true\"]",
            "damage_kinds = []",
        ),
        MODE,
        |problem| rule(problem, &CombatDataError::NoDamageKinds),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            r#"heal_scale = "healing_received_pct""#,
            r#"heal_scale = "heal_taken""#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, at: Place::Section("combat"), name } if name == "heal_taken"),
    ),
    flaw(MAP, Edit::Remove("grid"), MODE, |problem| {
        rule(problem, &VisionDataError::NoGrid)
    }),
    flaw(MAP, Edit::Remove("navigation"), MODE, |problem| {
        rule(problem, &NavigationDataError::NoPathingGrid)
    }),
    flaw(MAP, Edit::Set("grid.cell", r#""0""#), MODE, |problem| {
        rule(problem, &VisionDataError::Grid)
    }),
    flaw(
        MAP,
        Edit::Set("bounds", "{ min = [1, 0], max = [0, 1] }"),
        MODE,
        |problem| {
            read_fails(
                problem,
                "map/two_lanes/map.toml",
                "bounds need min below max",
            )
        },
    ),
    flaw(MAP, Edit::Set("units.0.pos", "[0, -69]"), MODE, |problem| {
        rule(problem, &ModeError::OutOfBounds)
    }),
    // A meter from the west lane's waypoint 1, at (−36, −36), less than the tower's body and the
    // widest walker's together.
    flaw(
        MAP,
        Edit::Set("units.2.pos", "[-36, -37]"),
        MODE,
        |problem| {
            let path = DeclaredName::new("west").unwrap();
            rule(problem, &MapProblem::WaypointBlocked { path, waypoint: 1 })
        },
    ),
    // A meter from the north spawn, at (0, −60).
    flaw(MAP, Edit::Set("units.2.pos", "[0, -59]"), MODE, |problem| {
        let marker = DeclaredName::new("north_spawn").unwrap();
        rule(problem, &MapProblem::MarkerBlocked { marker })
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
        |problem| rule(problem, &ActionProblem::Field(ActionField::Cost)),
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
        refused: |problem| {
            rule_at(
                problem,
                &Place::UnitTypes,
                &TypesError::Repeated("husk".to_owned()),
            )
        },
    },
    flaw(
        LASH_OUT,
        Edit::Replace("ctx.find(", "ctx.finds("),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownCtx(name), .. } if name == "finds"),
    ),
    // A call with `ctx` first is a call of `ctx`, as Rhai calls it, and a string literal index a
    // name of data, as a property is.
    flaw(
        LASH_OUT,
        Edit::Replace("ctx.find(", "finds(ctx, "),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownCtx(name), .. } if name == "finds"),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace("ctx.params.radius", r#"ctx.params["radiuss"]"#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: Param::NAMES, name, .. } if name == "radiuss"),
    ),
    // An AI's order in the mode's script.
    flaw(
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(
            "fn on_match_start(ctx) {",
            "fn on_match_start(ctx) {\n    ctx.order_reset();",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownCtx(name), .. } if name == "order_reset"),
    ),
    // Taking a bot's slot is a late join.
    flaw(
        MODE_DATA,
        Edit::Replace("late_join = true", "late_join = false"),
        MODE,
        |problem| rule(problem, &ModeDataError::BotTakeoverWithoutLateJoin),
    ),
    // An option whose default is none of its values; one of a param's name, which `ctx.params`
    // would read as one.
    flaw(
        MODE_DATA,
        Edit::Set("options.income_gold", "{ values = [8, 16], default = 4 }"),
        MODE,
        |problem| {
            read_fails(
                problem,
                "data/mode.toml",
                "an option's default is one of its values",
            )
        },
    ),
    flaw(
        MODE_DATA,
        Edit::Set("options.income_gold", "{ values = [8, 16], default = 8 }"),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("options", "income_gold"),
                &ModeDataError::OptionParam,
            )
        },
    ),
    // `[visibility]`: a name no setting has; a recipient life's audience holds whatever the
    // mode says.
    flaw(
        MODE_DATA,
        Edit::Set("visibility.fog", r#""all""#),
        MODE,
        |problem| read_fails(problem, "data/mode.toml", r#""fog" is no setting"#),
    ),
    flaw(
        MODE_DATA,
        Edit::Set("visibility.life", r#"["allies"]"#),
        MODE,
        |problem| {
            read_fails(
                problem,
                "data/mode.toml",
                "no mode adds allies to the audience of life, or takes them from it",
            )
        },
    ),
    // The match has no owner or team for a mode field to sync to.
    flaw(
        MODE_DATA,
        Edit::Replace(
            "first_blood = { type = \"bool\", default = false, sync = \"all\" }",
            "first_blood = { type = \"bool\", default = false, sync = \"team\" }",
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("state", "first_blood"),
                &ModeDataError::StateSync,
            )
        },
    ),
    // Names the script API plans: a `ctx` method and field, a handle's field, a hook and data
    // fields.
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
        |problem| rule_at(problem, &unit_type("tower_bolt"), &ProjectileError::Gravity),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace("ctx.params.radius", "ctx.params.radios"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: Param::NAMES, at: Place::Script(path), name } if path.to_string() == "scripts/lash_out.rhai" && name == "radios"),
    ),
    // A cooldown cut names an ability of the script's own package.
    flaw(
        LASH_OUT,
        Edit::Replace(
            r#"ctx.cut_cooldown(m.carrier, "lash_out""#,
            r#"ctx.cut_cooldown(m.carrier, "lash""#,
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: ActionId::NAMES, at: Place::Script(path), name } if path.to_string() == "scripts/lash_out.rhai" && name == "lash"),
    ),
    flaw(
        "heroes/cinder/scripts/wildfire.rhai",
        Edit::Replace(r#""kindle""#, r#""kindl""#),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Unknown { of: ModifierId::NAMES, name: id, .. } if id == "kindl"),
    ),
    flaw(
        "heroes/veil/scripts/dual_path.rhai",
        Edit::Replace(r#"stat("attack_damage")"#, r#"stat("attack_dmg")"#),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, name, .. } if name == "attack_dmg"),
    ),
    flaw(
        CREEP_AI,
        Edit::Replace("enemies:creep", "enemies:minion"),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: Filter::NAMES, name: filter, .. } if filter == "enemies:minion"),
    ),
    flaw(
        HUSK,
        Edit::Rename("stats.magic_resist", "spirit"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, name, .. } if name == "spirit"),
    ),
    flaw(
        HUSK,
        Edit::Rename("modifiers.withered.stats.magic_resist", "spirit"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, name, .. } if name == "spirit"),
    ),
    // A hold needs a toggle or a channel to hold its modifier while.
    flaw(
        RIME,
        Edit::Remove("actions.chill_arrows.toggle"),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "chill_arrows"),
                &ActionProblem::HoldAlone,
            )
        },
    ),
    // Only a point aim clamps to the range.
    flaw(
        "spells/data/loadout.toml",
        Edit::Set("actions.blink.targeting", r#""direction""#),
        "player-spells",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "blink"),
                &ActionProblem::ClampAims,
            )
        },
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
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, at: Place::Entry { table: "actions", name: _ }, name } if name == "spell_power"),
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
        |problem| stat_loop(problem, ["attack_damage", "spell_vamp"]),
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
        |problem| stat_loop(problem, ["attack_damage", "spell_vamp"]),
    ),
    flaw(
        "heroes/veil/data/avatar.toml",
        Edit::Replace("bonus = { attack_damage =", "bonus = { attack_dmg ="),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, at: Place::Entry { table: "modifiers", name: id }, name } if id == "dual_path" && name == "attack_dmg"),
    ),
];

/// Flaws of a pool, a match's start, a stat change, a modifier's owner, an aura, a way's params,
/// a name a script gives, a text and an engine stat.
static STAT_FLAWS: &[Flaw] = &[
    // Pools: each a unit lists, a cost names or a script reads is declared, and the life pool is
    // among a combatant's; no pool shares a name with a player resource, and no more than eight.
    flaw(
        HUSK,
        Edit::Replace(
            r#"pools = ["health", "mana"]"#,
            r#"pools = ["health", "rage"]"#,
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: PoolId::NAMES, at: Place::Avatar(_), name } if name == "rage"),
    ),
    flaw(
        HUSK,
        Edit::Replace(
            r#"pools = ["health", "mana"]"#,
            r#"pools = ["health", "mana", "mana"]"#,
        ),
        "hero-husk",
        |problem| {
            rule_of(problem, |at, error: &NamesListError| {
                let mana = NamesListError::Repeated(DeclaredName::new("mana").unwrap());
                matches!(at, Place::Avatar(_)) && *error == mana
            })
        },
    ),
    flaw(
        HUSK,
        Edit::Replace(r#"pools = ["health", "mana"]"#, r#"pools = ["mana"]"#),
        "hero-husk",
        |problem| {
            rule_of(problem, |at, error: &UnitStartError| {
                matches!(at, Place::Avatar(_)) && *error == UnitStartError::NoLifePool
            })
        },
    ),
    flaw(
        UNITS,
        Edit::Replace("pools = [\"health\"]\n", ""),
        MODE,
        |problem| {
            rule_at(
                problem,
                &unit_type("melee_creep"),
                &UnitStartError::NoLifePool,
            )
        },
    ),
    flaw(
        UNITS,
        Edit::Replace("combat = { on_death = \"stay\" }\n", ""),
        MODE,
        |problem| rule_at(problem, &unit_type("inhibitor"), &UnitStartError::NoCombat),
    ),
    flaw(
        UNITS,
        Edit::Replace(r#""tower", "true_sight"]"#, r#""tower", "avatar"]"#),
        MODE,
        |problem| {
            let avatar = UnitTypeError::EngineTag(DeclaredName::new("avatar").unwrap());
            rule_at(problem, &unit_type("tower"), &avatar)
        },
    ),
    flaw(
        HUSK,
        Edit::Replace(r#"tags = ["tank"]"#, r#"tags = ["area"]"#),
        "hero-husk",
        |problem| {
            rule_of(problem, |at, error: &UnitTypeError| {
                let area = UnitTypeError::EngineTag(DeclaredName::new("area").unwrap());
                matches!(at, Place::Avatar(_)) && *error == area
            })
        },
    ),
    flaw(
        RIME,
        Edit::Replace(r#"tags = ["slowed"]"#, r#"tags = ["projectile"]"#),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &entry("modifiers", "slow"),
                &ModifiersDataError::EngineTag(name("projectile")),
            )
        },
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
        |problem| {
            rule_of(problem, |at, error: &UnitStartError| {
                *at == unit_type("melee_creep") && matches!(error, UnitStartError::NotPositive(_))
            })
        },
    ),
    flaw(
        UNITS,
        Edit::Set("units.melee_creep.orders.think_ms", "400000000000000000"),
        MODE,
        |problem| rule(problem, &AiError::Time(TimeTooLarge)),
    ),
    flaw(
        CREEP_AI,
        Edit::Replace("fn on_think(ctx, unit) {", "fn think(ctx, unit) {"),
        MODE,
        |problem| {
            rule_of(problem, |at, error: &AiError| {
                matches!(at, Place::UnitType(_)) && *error == AiError::NoThink
            })
        },
    ),
    flaw(
        HUSK,
        Edit::Set("actions.lash_out.cooldown_ms.4", "400000000000000000"),
        "hero-husk",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "lash_out"),
                &ActionProblem::Time(TimeTooLarge),
            )
        },
    ),
    flaw(
        HUSK,
        Edit::Set("modifiers.withered.duration_ms", "-1"),
        "hero-husk",
        |problem| {
            rule_at(
                problem,
                &entry("modifiers", "withered"),
                &ModifierProblem::Time,
            )
        },
    ),
    // A stat change past what a number holds: 9 × 10¹² is past 2³⁹, as a number has 24 bits
    // of fraction in 64.
    flaw(
        HUSK,
        Edit::Set("modifiers.withered.stats.magic_resist", "-9000000000000"),
        "hero-husk",
        |problem| {
            rule_at(
                problem,
                &entry("modifiers", "withered"),
                &ModifierProblem::Overflow,
            )
        },
    ),
    flaw(
        CINDER,
        Edit::Set("units.eruption.area.delay_ms", "400000000000000000"),
        "hero-cinder",
        |problem| rule_at(problem, &unit_type("eruption"), &TimeTooLarge),
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
        refused: |problem| rule_at(problem, &unit_type("eruption"), &TimeTooLarge),
    },
    // A modifier is the passive of one owner of its package: Husk's avatar holds
    // `withering_touch`, and Lash Out `lash_out_guard`.
    flaw(
        HUSK,
        Edit::Set("actions.lash_out.passive_modifier", r#""withering_touch""#),
        "hero-husk",
        |problem| {
            shared_passive(
                problem,
                "withering_touch",
                BookPlace::UnitType(TypePlace::Avatar),
                "lash_out",
            )
        },
    ),
    flaw(
        HUSK,
        Edit::Set("actions.dread.passive_modifier", r#""lash_out_guard""#),
        "hero-husk",
        |problem| shared_passive(problem, "lash_out_guard", action_place("dread"), "lash_out"),
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
        refused: |problem| {
            shared_passive(
                problem,
                "warden_blessing",
                BookPlace::UnitType(TypePlace::Declared(name("tower"))),
                "tower_attack",
            )
        },
    },
    Flaw {
        file: "spells/data/loadout.toml",
        edit: Edit::Set("actions.blink.passive_modifier", r#""haste""#),
        also: &[(
            "spells/data/loadout.toml",
            Edit::Set("actions.haste.passive_modifier", r#""haste""#),
        )],
        package: "player-spells",
        refused: |problem| shared_passive(problem, "haste", action_place("blink"), "haste"),
    },
    // A modifier grants no engine tag: one that granted `avatar` would split who a player
    // commands between the type's own tags and the unit's.
    flaw(
        GALE,
        Edit::Set("modifiers.fair_wind.tags", r#"["avatar"]"#),
        "hero-gale",
        |problem| {
            rule_at(
                problem,
                &entry("modifiers", "fair_wind"),
                &ModifiersDataError::EngineTag(name("avatar")),
            )
        },
    ),
    // A projectile type holds no passive, as a delivery's spawn gives none: a type with one is
    // a type that stands, which no delivery names.
    flaw(
        GALE,
        Edit::Set("units.wind_bolt.passive", r#""wind_shield""#),
        "hero-gale",
        |problem| {
            rule_at(
                problem,
                &unit_type("wind_bolt"),
                &UnitTypeError::NotDelivery,
            )
        },
    ),
    // An aura's radius and a shield are never negative: as Gale's Fair Wind's value, as a rank
    // of the Wind Shield action's param its modifier reads, or as the modifier's own param.
    flaw(
        GALE,
        Edit::Set("modifiers.fair_wind.aura.radius", r#""-1.0""#),
        "hero-gale",
        |problem| {
            rule_at(
                problem,
                &entry("modifiers", "fair_wind"),
                &AppliedError::Sign(ModifierProblem::Negative),
            )
        },
    ),
    flaw(
        GALE,
        Edit::Set(
            "actions.wind_shield.params.shield.base",
            "[80, 120, -1, 200, 240]",
        ),
        "hero-gale",
        |problem| {
            rule_at(
                problem,
                &entry("modifiers", "wind_shield"),
                &AppliedError::Sign(ModifierProblem::Negative),
            )
        },
    ),
    flaw(
        GALE,
        Edit::Set("modifiers.wind_shield.params.shield", "-5"),
        "hero-gale",
        |problem| {
            rule_at(
                problem,
                &entry("modifiers", "wind_shield"),
                &AppliedError::Sign(ModifierProblem::Negative),
            )
        },
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
        refused: |problem| {
            rule_at(
                problem,
                &entry("modifiers", "stun"),
                &AppliedError::Param {
                    param: name("stun_ms"),
                    way: None,
                    problem: ParamProblem::ScalingTime,
                },
            )
        },
    },
    // A name a script gives the API, as the registry marks the argument: a tag, a track, a unit
    // type of the mode's scope, a team.
    flaw(
        MODE_SCRIPT,
        Edit::Replace(r#"unit.has_tag("core")"#, r#"unit.has_tag("cor")"#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: Tag::NAMES, name, .. } if name == "cor"),
    ),
    flaw(
        MODE_SCRIPT,
        Edit::Replace(
            r#"ctx.all_units_tagged("inhibitor")"#,
            r#"ctx.all_units_tagged("inhibitors")"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: Tag::NAMES, name, .. } if name == "inhibitors"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(r#"track = "level""#, r#"track = "levels""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: TrackId::NAMES, name, .. } if name == "levels"),
    ),
    flaw(
        MODE_SCRIPT,
        Edit::Replace(
            "    if unit.has_tag(\"inhibitor\") {\n",
            "    ctx.spawn_unit(\"grasping_wraps\", \"north\", unit.pos);\n    if unit.has_tag(\"inhibitor\") {\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, name, .. } if name == "grasping_wraps"),
    ),
    flaw(
        MODE_SCRIPT,
        Edit::Replace(
            "    if unit.has_tag(\"inhibitor\") {\n",
            "    ctx.spawn_unit(\"hero-husk\", \"west\", unit.pos);\n    if unit.has_tag(\"inhibitor\") {\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: Team::NAMES, name, .. } if name == "west"),
    ),
    // Human text: each message an avatar names has a value in its own language's file, and each
    // file under `locale/` is `<language>.ftl`, parses, defines no message twice, and in another
    // language defines only the own file's messages.
    flaw(
        HUSK,
        Edit::Replace(r#"name = "hero-name""#, r#"name = "hero-nam""#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: MessageId::NAMES, at: Place::Avatar(package), name } if package == "hero-husk" && name == "hero-nam"),
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
        |problem| matches!(problem, LoadProblem::Unknown { of: MessageId::NAMES, name, .. } if name == "hero-name"),
    ),
    // The names players see: the mode's, each team's, each choice's and each of an avatar's or a
    // loadout's actions, each a message of its package's own language.
    flaw(
        MODE_TEXT,
        Edit::Replace("mode-name = Three on Three\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: MessageId::NAMES, at: Place::Mode, name } if name == "mode-name"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("[teams.camps]\nname = \"team-camps\"\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::UnnamedTeam(team) if team == "camps"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            "[teams.north]",
            "[teams.river]\nname = \"team-north\"\n\n[teams.north]",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: Team::NAMES, at: Place::Mode, name } if name == "river"),
    ),
    flaw(
        MODE_TEXT,
        Edit::Replace("choice-hero = Hero\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: MessageId::NAMES, at: Place::Choice(choice), name } if choice == "hero" && name == "choice-hero"),
    ),
    flaw(
        HUSK,
        Edit::Remove("actions.dread.name"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::UnnamedAction(action) if action == "dread"),
    ),
    flaw(
        "spells/data/loadout.toml",
        Edit::Remove("actions.blink.name"),
        "player-spells",
        |problem| matches!(problem, LoadProblem::UnnamedAction(action) if action == "blink"),
    ),
    flaw(
        HUSK_TEXT,
        Edit::Replace("action-dread = Dread\n", ""),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: MessageId::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "dread" && name == "action-dread"),
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
                "1099511627776 is past what a number holds",
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
                "1099511627776 is past what a number holds",
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
        |problem| {
            read_fails(
                problem,
                "map/two_lanes/map.toml",
                r#""west lane" is not a name"#,
            )
        },
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
        |problem| matches!(problem, LoadProblem::Unknown { of: CostTarget::NAMES, at: Place::Entry { table: "actions", name: id }, name } if id == "lash_out" && name == "rage"),
    ),
    flaw(
        "heroes/veil/scripts/dusk_mark.rhai",
        Edit::Replace(r#""energy""#, r#""rage""#),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::Unknown { of: PoolId::NAMES, at: Place::Script(_), name } if name == "rage"),
    ),
    flaw(
        "heroes/veil/scripts/dusk_mark.rhai",
        Edit::Replace(r#"is_action("dusk_mark")"#, r#"is_action("dusk")"#),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::Unknown { of: ActionId::NAMES, at: Place::Script(_), name } if name == "dusk"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("life = \"health\"\n", ""),
        MODE,
        |problem| rule(problem, &CombatDataError::NoLifePool),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(r#"life = "health""#, r#"life = "hp""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: PoolId::NAMES, at: Place::Section("combat"), name } if name == "hp"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("[pools.mana]\nmax = \"mana\"", "[pools.mana]\nmax = \"mp\""),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, at: Place::Entry { table: "pools", name: pool }, name } if pool.as_str() == "mana" && name == "mp"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            "[pools.energy]",
            "[pools.p1]\nmax = \"mana\"\n[pools.p2]\nmax = \"mana\"\n[pools.p3]\nmax = \"mana\"\n[pools.p4]\nmax = \"mana\"\n[pools.p5]\nmax = \"mana\"\n[pools.p6]\nmax = \"mana\"\n[pools.energy]",
        ),
        MODE,
        |problem| rule(problem, &NamesListError::TooMany),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(r#"resources = ["gold"]"#, r#"resources = ["gold", "gold"]"#),
        MODE,
        |problem| {
            let gold = NamesListError::Repeated(DeclaredName::new("gold").unwrap());
            rule_at(problem, &Place::Section("resources"), &gold)
        },
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(r#"resources = ["gold"]"#, r#"resources = ["gold", "mana"]"#),
        MODE,
        |problem| {
            let mana = NamesListError::Repeated(DeclaredName::new("mana").unwrap());
            rule_at(problem, &Place::Section("resources"), &mana)
        },
    ),
    // An engine stat is declared like any other: one the units carry and the mode does not
    // declare fails.
    flaw(
        MODE_DATA,
        Edit::Replace("move_speed = {}\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, name, .. } if name == "move_speed"),
    ),
    flaw(
        MODE_DATA,
        Edit::Set("stats.cooldown_reduction", r#"{ min = 1, max = "0.4" }"#),
        MODE,
        |problem| read_fails(problem, "data/mode.toml", "a stat's min passes its max"),
    ),
    // An armor of a damage kind the mode does not declare, or a unit type of an armor the mode
    // does not declare, fails.
    flaw(
        MODE_DATA,
        Edit::Set("combat.armors.plate", r#"{ cold = "0.5" }"#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: DamageKind::NAMES, name, .. } if name == "cold"),
    ),
    flaw(
        UNITS,
        Edit::Set("units.inhibitor.combat.armor", r#""plate""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: Armor::NAMES, name, .. } if name == "plate"),
    ),
    flaw(
        "heroes/veil/scripts/whirling_blades.rhai",
        Edit::Replace(r#""physical""#, r#""fire""#),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::Unknown { of: DamageKind::NAMES, name: kind, .. } if kind == "fire"),
    ),
    flaw(
        GALE,
        Edit::Replace(
            r#"{ param = "bonus_speed" }"#,
            r#"{ param = "bonus_speeds" }"#,
        ),
        "hero-gale",
        |problem| matches!(problem, LoadProblem::Unknown { of: Param::NAMES, at: Place::Entry { table: "modifiers", name: id }, name } if id == "gust_speed" && name == "bonus_speeds"),
    ),
    flaw(
        "heroes/kensho/data/avatar.toml",
        Edit::Replace(r#"hold = "still_mind""#, r#"hold = "still_mindful""#),
        "hero-kensho",
        |problem| matches!(problem, LoadProblem::Unknown { of: ModifierId::NAMES, at: Place::Entry { table: "actions", name: ability }, name: id } if ability == "still_mind" && id == "still_mindful"),
    ),
    flaw(
        GALE,
        Edit::Replace(r#"affects = "allies""#, r#"affects = "allies:friends""#),
        "hero-gale",
        |problem| matches!(problem, LoadProblem::Unknown { of: Filter::NAMES, name: filter, .. } if filter == "allies:friends"),
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
        |problem| {
            rule_of(problem, |at, error: &SlotError| {
                let twice = SlotError::Repeated(DeclaredName::new("grasping_wraps").unwrap());
                matches!(at, Place::Avatar(_)) && *error == twice
            })
        },
    ),
    flaw(
        MAP,
        Edit::Replace(r#"unit_type = "core""#, r#"unit_type = "cor""#),
        MODE,
        |problem| {
            rule(
                problem,
                &ModeError::UnknownUnitType(DeclaredName::new("cor").unwrap()),
            )
        },
    ),
    flaw(
        MAP,
        Edit::Replace(r#"path = "west""#, r#"path = "north""#),
        MODE,
        |problem| {
            rule(
                problem,
                &ModeError::UnknownPath(DeclaredName::new("north").unwrap()),
            )
        },
    ),
];

/// Flaws of the map, its layers, the slot kinds, production, gathering, a use, a player
/// resource, a player modifier, a relation and a loadout.
static MAP_FLAWS: &[Flaw] = &[
    // A marker tag a script names is some marker's; a point fits the map's metric; a region is a
    // box within the bounds, of a marker with no point; a placed unit walks only a path it
    // names; marker names differ.
    flaw(
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(r#"ctx.map.markers("camp")"#, r#"ctx.map.markers("camps")"#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: MarkerTag::NAMES, name: tag, .. } if tag == "camps"),
    ),
    flaw(
        MAP,
        Edit::Set("markers.0.pos", "[0, 0, -60]"),
        MODE,
        |problem| rule(problem, &ModeError::PointShape),
    ),
    Flaw {
        file: MAP,
        edit: Edit::Remove("markers.2.pos"),
        also: &[(
            MAP,
            Edit::Set("markers.2.region", "{ min = [-20, -14], max = [-16, -70] }"),
        )],
        package: MODE,
        refused: |problem| {
            rule(
                problem,
                &ModeError::Region(DeclaredName::new("camp1").unwrap()),
            )
        },
    },
    flaw(
        MAP,
        Edit::Set("markers.2.region", "{ min = [-20, -14], max = [-16, -10] }"),
        MODE,
        |problem| {
            rule(
                problem,
                &ModeError::Region(DeclaredName::new("camp1").unwrap()),
            )
        },
    ),
    flaw(
        MAP,
        Edit::Set("units.2.path", r#""middle""#),
        MODE,
        |problem| {
            rule(
                problem,
                &ModeError::UnknownPath(DeclaredName::new("middle").unwrap()),
            )
        },
    ),
    flaw(
        MAP,
        Edit::Set("units.0.from", r#""start""#),
        MODE,
        |problem| {
            rule(
                problem,
                &ModeError::NoPathToWalk(DeclaredName::new("core").unwrap()),
            )
        },
    ),
    flaw(
        MAP,
        Edit::Set("markers.3.name", r#""camp1""#),
        MODE,
        |problem| {
            rule(
                problem,
                &ModeError::RepeatedMarker(DeclaredName::new("camp1").unwrap()),
            )
        },
    ),
    // The layers have names of their own, a unit type moves on one of them, and they are
    // navigation's.
    flaw(
        MODE_DATA,
        Edit::Set("navigation.layers", r#"["ground", "ground"]"#),
        MODE,
        |problem| {
            let ground = NamesListError::Repeated(DeclaredName::new("ground").unwrap());
            rule_at(problem, &Place::Section("navigation"), &ground)
        },
    ),
    flaw(
        UNITS,
        Edit::Set("units.melee_creep.collision.layer", r#""air""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: Layer::NAMES, at: Place::UnitType(unit_type), name: layer } if unit_type == "melee_creep" && layer.as_str() == "air"),
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
                    at: Place::Section("navigation")
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
        |problem| matches!(problem, LoadProblem::Unknown { of: SlotKind::NAMES, at: Place::Avatar(avatar), name } if avatar == "hero-husk" && name == "ultimates"),
    ),
    flaw(
        HUSK,
        Edit::Replace(
            r#""grasping_wraps", "dread", "lash_out""#,
            r#""grasping_wraps", "lash_out""#,
        ),
        "hero-husk",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "dread"),
                &ActionProblem::Unslotted,
            )
        },
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("name = \"ultimate\"", "name = \"basic\""),
        MODE,
        |problem| read_fails(problem, "data/mode.toml", "the slot kind `basic` twice"),
    ),
    flaw(
        MODE_DATA,
        Edit::Remove("choices.spells.slot"),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("choices", "spells"),
                &ModeDataError::ChoiceSlot,
            )
        },
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            "unique = true\n\n[choices.spells]",
            "unique = true\nslot = \"spell\"\n\n[choices.spells]",
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("choices", "hero"),
                &ModeDataError::ChoiceSlot,
            )
        },
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("slot = \"spell\"", "slot = \"spells\""),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: SlotKind::NAMES, at: Place::Entry { table: "choices", name: choice }, name } if choice.as_str() == "spells" && name == "spells"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            "slot = \"spell\"",
            "slot = \"spell\"\n\n[choices.more]\nname = \"choice-spells\"\noffers = \"loadout\"\nslot = \"basic\"",
        ),
        MODE,
        |problem| rule(problem, &ModeDataError::LoadoutRanks),
    ),
    flaw(
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(
            r#"ctx.chosen(player, "spells")"#,
            r#"ctx.chosen(player, "spell")"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: ChoiceId::NAMES, name, .. } if name == "spell"),
    ),
    flaw(
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(r#"ctx.after("pick_end""#, r#"ctx.after("pick_ends""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: TimerId::NAMES, name, .. } if name == "pick_ends"),
    ),
    flaw(
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(
            r#"ctx.grant(unit, "spell","#,
            r#"ctx.grant(unit, "spells","#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: SlotKind::NAMES, name, .. } if name == "spells"),
    ),
    // A train is production's, names a unit type of the mode and takes no target, and sits on a
    // unit type with a queue; no other kind names a unit type.
    flaw(
        MODE_DATA,
        Edit::Replace("[actions.melee_creep_attack]", RECRUIT),
        MODE,
        |problem| matches!(problem, LoadProblem::Undeclared { capability: Capability::Production, at: Place::Entry { table: "actions", name: action } } if action == "recruit"),
    ),
    // A train needs the name players see on its producer's button.
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace("[actions.melee_creep_attack]", RECRUIT),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::UnnamedAction(action) if action == "recruit"),
    },
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
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, name, .. } if name == "knight"),
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
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "recruit"),
                &ProductionDataError::TrainAims,
            )
        },
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
        refused: |problem| {
            let recruit = ProductionDataError::NoQueue(DeclaredName::new("recruit").unwrap());
            rule_at(problem, &unit_type("tower"), &recruit)
        },
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
        |problem| {
            rule_at(
                problem,
                &unit_type("melee_creep"),
                &NavigationDataError::BoxWalks,
            )
        },
    ),
    flaw(
        UNITS,
        Edit::Replace(
            r#"tags = ["ward", "stealthed"]"#,
            "tags = [\"ward\", \"stealthed\"]\ncollision = { box = [\"0.5\", \"0.5\"] }",
        ),
        MODE,
        |problem| {
            rule_of(problem, |_, error: &ActionProblem| {
                matches!(
                    error,
                    ActionProblem::Effect {
                        rule: EffectRule::SpawnBox,
                        ..
                    }
                )
            })
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
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "recruit"),
                &ProductionDataError::TrainStands,
            )
        },
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
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "recruit" && name == "tower_bolt"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.recruit]\nkind = \"train\"\ntargeting = \"none\"\nunit_type = \"melee_creep\"\nrequires = { units = [\"tower\"], modifiers = [\"drill\"] }\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: ModifierId::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "recruit" && name == "drill"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.recruit]\nkind = \"train\"\ntargeting = \"none\"\nunit_type = \"melee_creep\"\nrequires = { units = [\"keep\"] }\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "recruit" && name == "keep"),
    },
    // A unit type's passive is a modifier of its package.
    flaw(
        UNITS,
        Edit::Set("units.tower.passive", r#""warden_blesing""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: ModifierId::NAMES, at: Place::UnitType(unit_type), name } if unit_type == "tower" && name == "warden_blesing"),
    ),
    flaw(
        HUSK,
        Edit::Replace("[actions.dread]\n", "[actions.dread]\nrequires = {}\n"),
        "hero-husk",
        |problem| read_fails(problem, "data/avatar.toml", "unknown field `requires`"),
    ),
    Flaw {
        file: UNITS,
        edit: Edit::Replace(
            r#"slots = { weapon = ["tower_attack"] }"#,
            "slots = { weapon = [\"tower_attack\"] }\nsupply = { provides = 10 }",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &unit_type("tower"),
                &ProductionDataError::SupplyUncounted,
            )
        },
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
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "raise"),
                &ProductionDataError::BuildAims,
            )
        },
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"tower\"\nconstruct = \"alone\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "raise"),
                &ProductionDataError::NoBox,
            )
        },
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"dragon\"\nconstruct = \"alone\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, name, .. } if name == "dragon"),
    },
    // A delivery type is no unit type of the mode that a build raises.
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"caster_creep_bolt\"\nconstruct = \"alone\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "raise" && name == "caster_creep_bolt"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"yard\"\nconstruct = \"alone\"\nstart_life = \"0\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "raise"),
                &ProductionDataError::StartLife,
            )
        },
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"yard\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| read_fails(problem, "data/mode.toml", "missing field `construct`"),
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
                    of: Filter::NAMES,
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
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "raise"),
                &ProductionDataError::StartLife,
            )
        },
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"global\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"yard\"\nconstruct = \"alone\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| {
            let global = ActionProblem::Global;
            rule_at(problem, &entry("actions", "raise"), &global)
        },
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
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "mine"),
                &ProductionDataError::GatherAims,
            )
        },
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.mine]\nkind = \"gather\"\nrange = \"global\"\nwindup_ms = 1000\ntargeting = \"neutrals\"\nresource = \"gold\"\ntake = 5\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, GOLD_MINE)],
        package: MODE,
        refused: |problem| {
            let global = ActionProblem::Global;
            rule_at(problem, &entry("actions", "mine"), &global)
        },
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.mine]\nkind = \"gather\"\nrange = \"1\"\nwindup_ms = 1000\ntargeting = \"neutrals\"\nresource = \"gold\"\ntake = 5\nbounce = \"-1\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, GOLD_MINE)],
        package: MODE,
        refused: |problem| read_fails(problem, "data/mode.toml", "a distance is a number from 0"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.mine]\nkind = \"gather\"\nrange = \"1\"\nwindup_ms = 1000\ntargeting = \"neutrals\"\nresource = \"silver\"\ntake = 5\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, GOLD_MINE)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: ResourceId::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "mine" && name == "silver"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.mine]\nkind = \"gather\"\nrange = \"1\"\nwindup_ms = 1000\ntargeting = \"neutrals\"\nresource = \"gold\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, GOLD_MINE)],
        package: MODE,
        refused: |problem| read_fails(problem, "data/mode.toml", "missing field `take`"),
    },
    Flaw {
        file: UNITS,
        edit: Edit::Replace(
            "[units.tower]\n",
            "[units.mine]\nnode = { resource = \"silver\", amount = 1500 }\n\n[units.tower]\n",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: ResourceId::NAMES, at: Place::UnitType(unit), name } if unit == "mine" && name == "silver"),
    },
    Flaw {
        file: UNITS,
        edit: Edit::Replace(
            "[units.tower]\n",
            "[units.depot]\ndrop_off = { resources = [\"gold\", \"silver\"] }\n\n[units.tower]\n",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: ResourceId::NAMES, at: Place::UnitType(unit), name } if unit == "depot" && name == "silver"),
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
        |problem| read_fails(problem, "data/avatar.toml", "unknown field `unit_type`"),
    ),
    // The release runs no action of the kind `enter` yet; an action sits in
    // slot kinds of one count of ranks.
    flaw(
        HUSK,
        Edit::Replace("[actions.dread]\n", "[actions.dread]\nkind = \"enter\"\n"),
        "hero-husk",
        |problem| {
            let message = "the release runs no action of the kind `enter`";
            read_fails(problem, "data/avatar.toml", message)
        },
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
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "taunt"),
                &ActionProblem::SlottedRanks,
            )
        },
    },
    // A script adds only to a player resource the mode declares.
    flaw(
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(
            r#"add_resource(killer.owner, "gold", gold)"#,
            r#"add_resource(killer.owner, "silver", gold)"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: ResourceId::NAMES, name, .. } if name == "silver"),
    ),
    // A type's bounty pays a player resource the mode declares.
    flaw(
        UNITS,
        Edit::Replace(
            r#"bounty = { resource = "gold", amount = 20 }"#,
            r#"bounty = { resource = "silver", amount = 20 }"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: ResourceId::NAMES, name, .. } if name == "silver"),
    ),
    // A player modifier's filter names a tag the mode has.
    flaw(
        MODE_DATA,
        Edit::Replace(
            "[modifiers.warden_blessing]\n",
            "[modifiers.warden_blessing]\naffects = \"allies:totem\"\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: Filter::NAMES, name: filter, .. } if filter == "allies:totem"),
    ),
    // A relation names two of the mode's teams, a pair once.
    flaw(
        MODE_DATA,
        Edit::Replace(
            "resources = [\"gold\"]\n",
            "resources = [\"gold\"]\n\n[[relations]]\nteams = [\"north\", \"east\"]\nrelation = \"neutral\"\n",
        ),
        MODE,
        |problem| {
            rule(
                problem,
                &ModeError::UnknownTeam(DeclaredName::new("east").unwrap()),
            )
        },
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            "resources = [\"gold\"]\n",
            "resources = [\"gold\"]\n\n[[relations]]\nteams = [\"north\", \"camps\"]\nrelation = \"neutral\"\n\n[[relations]]\nteams = [\"camps\", \"north\"]\nrelation = \"friendly\"\n",
        ),
        MODE,
        |problem| {
            let name = |name| DeclaredName::new(name).unwrap();
            rule(
                problem,
                &ModeError::RepeatedRelation(name("camps"), name("north")),
            )
        },
    ),
    flaw(
        "heroes/kensho/data/avatar.toml",
        Edit::Replace(r#"targeting = "enemies""#, r#"targeting = "enemies:totem""#),
        "hero-kensho",
        |problem| matches!(problem, LoadProblem::Unknown { of: Filter::NAMES, name: filter, .. } if filter == "enemies:totem"),
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

/// Each of `flaws` fails the load of the packages it edits, in its package, with its problem.
fn fails_each(flaws: &[Flaw]) {
    for flaw in flaws {
        let edits = [(flaw.file, flaw.edit)]
            .into_iter()
            .chain(flaw.also.iter().copied());
        let Err(error) = load(&edited(edits)) else {
            panic!("{flaw:?} loads");
        };
        let LoadError { package, problem } = &error;
        let named = match package {
            PackageRef::Name(name) => name == flaw.package,
            PackageRef::Dir(dir) => dir.ends_with(flaw.package),
            PackageRef::PackageFingerprint(_) => false,
        };
        assert!(named && (flaw.refused)(problem), "{flaw:?}: {error:?}");
    }
}

#[test]
fn every_flaw_of_the_data_or_a_delivery_fails_its_load_with_its_own_problem() {
    fails_each(DATA_FLAWS);
}

#[test]
fn every_flaw_of_a_script_fails_its_load_with_its_own_problem() {
    fails_each(SCRIPT_FLAWS);
}

#[test]
fn every_flaw_of_an_action_or_a_loop_of_stats_fails_its_load_with_its_own_problem() {
    fails_each(ACTION_FLAWS);
}

#[test]
fn every_flaw_of_a_stat_a_modifier_or_a_text_fails_its_load_with_its_own_problem() {
    fails_each(STAT_FLAWS);
    // `has_modifier` only names a modifier: Rime's passive's script may ask for the Slow it
    // gives no param of, where an application of it is a flaw.
    let asks = Edit::Replace(
        "m.stacks += 1;",
        r#"if m.carrier.has_modifier("slow") { m.stacks += 1; }"#,
    );
    assert!(load(&edited([(STILLNESS, asks)])).is_ok());
}

#[test]
fn every_flaw_of_the_map_or_the_economy_fails_its_load_with_its_own_problem() {
    fails_each(MAP_FLAWS);
}

#[test]
fn a_mode_holds_kinds_and_resources_to_their_limits_and_a_hero_is_no_mode() {
    // The three kinds and 253 more are 256, all a byte tells apart; one more fails.
    let kinds = |count: usize| {
        let text = PackageTree::of(TestMode::Moba3v3)
            .text(MODE_DATA)
            .to_owned();
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
        load(&edited([(MODE_DATA, Edit::Create(text))]))
    };
    assert!(kinds(253).is_ok());
    let error = kinds(254).unwrap_err();
    assert!(
        rule_at(
            &error.problem,
            &Place::Section("combat"),
            &NamesListError::TooMany
        ),
        "{error:?}"
    );
    // Gold and 255 more are 256, all a resource id counts; one more fails.
    let resources = |count: usize| {
        let mut more = String::new();
        for at in 0..count {
            write!(more, ", \"r{at}\"").unwrap();
        }
        let text = PackageTree::of(TestMode::Moba3v3).text(MODE_DATA).replacen(
            r#"resources = ["gold"]"#,
            &format!(r#"resources = ["gold"{more}]"#),
            1,
        );
        let text: &'static str = Box::leak(text.into_boxed_str());
        load(&edited([(MODE_DATA, Edit::Create(text))]))
    };
    assert!(resources(255).is_ok());
    let error = resources(256).unwrap_err();
    assert!(
        rule_at(
            &error.problem,
            &Place::Section("resources"),
            &NamesListError::TooMany
        ),
        "{error:?}"
    );

    // The flaws hold each data field and tag property the release plans: `loot` and `noise` as
    // an action's effects, the others as fields.
    let api = Release::script_api();
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
    let husk = PackageDir::new(moba().join("heroes/husk"))
        .mode_packages(Some(&two_lanes()))
        .unwrap_err();
    assert!(matches!(*husk.problem, LoadProblem::WrongKind), "{husk}");
}

/// Every ground point of `map` raised to `[x, y, z]`, and the map made spatial: each point of a
/// `pos` and a `points` list to y = 0, and a region from y = 0 to 10, as a spatial region is a
/// box with height; not the bounds, which stay on the ground plane.
fn raised(map: &str) -> String {
    fn raise(value: &mut TomlNode, height: Option<i64>) {
        match value {
            TomlNode::Table(table) => {
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
            TomlNode::List(items) => match height {
                Some(y) if !matches!(items[0], TomlNode::List(_)) => {
                    items.insert(1, TomlNode::Integer(y));
                }
                _ => {
                    for item in items {
                        raise(item, height);
                    }
                }
            },
            TomlNode::Integer(_) | TomlNode::Float(_) | TomlNode::Text(_) | TomlNode::Flag(_) => {}
        }
    }
    let mut document = TomlDocument::parse(map);
    let table = document.table_mut();
    for (key, value) in table.iter_mut() {
        if key != "bounds" {
            raise(value, None);
        }
    }
    table.insert("metric".to_owned(), TomlNode::Text("spatial".to_owned()));
    document.text()
}

#[test]
fn a_build_and_a_gather_with_their_nodes_and_drop_offs_load() {
    // A build of a yard, a box with no life pool, so no start life, at a builders' rate table,
    // placed away from enemies; a gather of gold from a mine, which the yard takes back: the load
    // and its books take them all.
    let raise = "[actions.raise]\nname = \"action-raise\"\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"yard\"\nconstruct = { builders = [\"1\", \"1.5\"] }\ncancel_refund = \"0.75\"\nplacement = { away = [{ filter = \"enemies\", distance = \"4\" }] }\n\n[actions.melee_creep_attack]";
    let edits = [
        (
            MODE_DATA,
            Edit::Replace("[actions.melee_creep_attack]", raise),
        ),
        (MANIFEST, PRODUCTION),
        (UNITS, YARD),
        (
            MODE_TEXT,
            Edit::Replace(
                "mode-name = Three on Three\n",
                "mode-name = Three on Three\naction-raise = Raise\n",
            ),
        ),
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
    let loaded = load(&edited(edits));
    assert!(loaded.is_ok(), "{:?}", loaded.err());
}

#[test]
fn a_mode_loads_the_map_its_session_names() {
    // The 3v3 with a second map, its own raised to the spatial metric: each name loads its map,
    // and a third is no map of the mode.
    let map = PackageTree::of(TestMode::Moba3v3).text(MAP).to_owned();
    let raised = raised(&map);
    let second = MapName::parse("second").unwrap();
    let two = edited([("modes/3v3/map/second/map.toml", Edit::Create(&raised))]);
    let planar = load(&two).unwrap();
    assert_eq!(planar.map_name(), &two_lanes());
    assert_eq!(planar.map().metric(), Metric::Planar);
    let spatial = two.mode_packages(Some(&second)).unwrap();
    assert_eq!(spatial.map_name(), &second);
    assert_eq!(spatial.map().metric(), Metric::Spatial);
    let third = MapName::parse("third").unwrap();
    let error = two.mode_packages(Some(&third)).unwrap_err();
    assert!(matches!(&*error.problem, LoadProblem::NoMap(map) if *map == third));

    // With no map named, the only map; of two, neither, and the error names both in order.
    let only = edited([]).mode_packages(None).unwrap();
    assert_eq!(only.map_name(), &two_lanes());
    let error = two.mode_packages(None).unwrap_err();
    assert!(
        matches!(&*error.problem, LoadProblem::MapNeeded(maps) if *maps == [second, two_lanes()]),
        "{error:?}"
    );
    assert_eq!(
        error.problem.to_string(),
        "name one of the mode's maps: second, two_lanes"
    );
    // A directory under `map/` whose name no map can have fails the load.
    let upper = edited([("modes/3v3/map/Upper/map.toml", Edit::Create(&raised))]);
    assert!(matches!(
        &*load(&upper).unwrap_err().problem,
        LoadProblem::MapDirName(name) if name == "Upper"
    ));
}

#[test]
fn a_maps_height_grid_loads_with_it_and_one_that_does_not_decode_fails_the_load() {
    // The 3v3's map has no height grid. A grid of 2 × 2 samples at its `heights.bin` loads with
    // its packages, and bytes that decode as none fail the load at that file.
    let heights = "modes/3v3/map/two_lanes/heights.bin";
    assert_eq!(load(&edited([])).unwrap().heights(), None);
    let grid = HeightGrid::new([Num::ZERO; 2], Num::ONE, Num::ONE, 2, vec![0, 1, 2, 3]).unwrap();
    let mut tree = PackageTree::of(TestMode::Moba3v3);
    tree.put(heights, Binary::encode(&grid));
    assert_eq!(load(&tree.dir(MODE_DIR)).unwrap().heights(), Some(&grid));
    tree.put(heights, vec![0xFF]);
    let error = load(&tree.dir(MODE_DIR)).unwrap_err();
    assert!(
        matches!(&*error.problem, LoadProblem::Content(ContentError::Binary { path, .. }) if path.as_str() == "map/two_lanes/heights.bin"),
        "{error:?}"
    );
}

#[test]
fn a_box_lies_only_on_a_planar_map() {
    // The 3v3 with its towers' bodies boxes: a planar map loads, and the same map made spatial
    // fails, at the first tower, as a box lies on the ground plane.
    let tower = Edit::Replace(
        r#"collision = { radius = "0.9" }"#,
        r#"collision = { box = ["1.8", "1.8"] }"#,
    );
    assert!(load(&edited([(UNITS, tower)])).is_ok());
    let map = PackageTree::of(TestMode::Moba3v3).text(MAP).to_owned();
    let raised = raised(&map);
    let spatial = [(MAP, Edit::Create(&raised)), (UNITS, tower)];
    let error = load(&edited(spatial)).unwrap_err();
    assert!(
        rule_at(
            &error.problem,
            &unit_type("tower"),
            &UnitTypeError::BoxOnSpatial
        ),
        "{error:?}"
    );
    // The raised map alone loads: the points it raised are its only change.
    let alone = load(&edited([(MAP, Edit::Create(&raised))]));
    assert!(alone.is_ok(), "{:?}", alone.err());
}

#[test]
fn a_build_or_a_gather_without_navigation_fails_its_load() {
    // The ruled mode runs no navigation, and production without it runs no build, whose box only
    // navigation tests for room and whose worker walks, nor a gather, whose worker walks.
    let raise = "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"yard\"\nconstruct = \"alone\"\n";
    let mine = "[actions.mine]\nkind = \"gather\"\nrange = \"1\"\nwindup_ms = 1000\ntargeting = \"neutrals\"\nresource = \"money\"\ntake = 5\n";
    for (action, data) in [("raise", raise), ("mine", mine)] {
        let mut tree = PackageTree::of(TestMode::Ruled);
        tree.edit(
            "modes/ruled/manifest.toml",
            Edit::Replace("capabilities = []", r#"capabilities = ["production"]"#),
        )
        .edit(
            "modes/ruled/data/mode.toml",
            Edit::Replace(
                "\n[teams.player]",
                "resources = [\"money\"]\n\n[teams.player]",
            ),
        )
        .edit("modes/ruled/data/mode.toml", Edit::Append(data))
        .edit(
            "modes/ruled/data/units.toml",
            Edit::Append("[units.yard]\ncollision = { box = [\"4\", \"2\"] }\n"),
        );
        let error = tree.load().unwrap_err();
        assert!(
            matches!(
                &*error.problem,
                LoadProblem::Undeclared {
                    capability: Capability::Navigation,
                    at: Place::Entry { table: "actions", name: at },
                } if at == action
            ),
            "{action}: {error:?}"
        );
    }
}
