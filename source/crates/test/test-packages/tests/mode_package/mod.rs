//! Each flaw a package can have fails the load of the MOBA packages with its own problem.

use std::fmt::Write;
use std::num::NonZeroU32;
use std::sync::Arc;

use campfire_actions::{
    ActionProblem, EffectData, EffectTo, LaunchLists, ListedEffect, PipelineField,
};
use campfire_areas::LandEffectData;
use campfire_capabilities::Release;
use campfire_codec::Binary;
use campfire_codec::internals::{TomlDocument, TomlNode};
use campfire_combat::{CombatDataError, HealEffectData};
use campfire_common::{DeclaredName, MapName};
use campfire_declarations::{BookPlace, NamesListError};
use campfire_geometry::Metric;
use campfire_math::Num;
use campfire_mode::{ModeData, ModeDataError, ModeError, PlayerControl};
use campfire_package::{LoadProblem, ModePackages, ModeRead, PackageRef, Place, ScriptProblem};
use campfire_package_disk::PackageDir;
use campfire_package_format::ContentError;
use campfire_progression::TrackError;
use campfire_projectiles::{ProjectileDeliveryError, ProjectileError};
use campfire_script::internals::Undefined;
use campfire_script::{Audience, ParamProblem, Status};
use campfire_sim::{Capability, MatchRate};
use campfire_stats::{AppliedError, PurgeEffectData, StatGraphError, Way};
use campfire_test_packages::{Edit, PackageTree, TestMode};
use campfire_units::{HeightGrid, TypesError, UnitTypeError};
use campfire_values::{Number, PackageName, Scalar, Stat};

use crate::moba::{edited, load, moba, two_lanes};
use crate::module_rules::{entry, rule, rule_at, unit_type};

/// A flaw, the package it is in, and the problem it fails the load with.
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
fn a_choice_of_values_lists_fewer_than_a_player_slot_picks_none() {
    // A player slot pick's byte holds a value's place, 255 its none: 254 values load, 255 do not.
    let side = |count: usize| {
        let values: Vec<String> = (0..count).map(|at| format!("\"v{at}\"")).collect();
        format!(
            "[choices.side]\nname = \"choice-hero\"\noffers = \"values\"\nvalues = [{}]\n\n\
             [choices.spells]",
            values.join(", ")
        )
    };
    let edit = |text: &str| {
        load(&edited([(
            MODE_DATA,
            Edit::Replace("[choices.spells]", text),
        )]))
    };
    assert!(edit(&side(254)).is_ok());
    let refused = edit(&side(255)).unwrap_err();
    assert!(
        rule_at(
            &refused.problem,
            &entry("choices", "side"),
            &ModeDataError::ChoiceValues
        ),
        "{refused:?}"
    );
}

#[test]
fn a_weapon_or_an_action_at_a_unit_or_a_point_may_fly_a_curve_or_steer_and_one_along_a_direction_not()
 {
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
    // The tower's weapon may be steered, and the husk's wraps may not.
    let steer = r#"steer = { launch_speed = "2", acceleration = "10", turn_rate = "90", lock = "3", fuel_ms = 3000 }"#;
    let missile = format!(r#"{{ speed = "5", {steer} }}"#);
    let edit = Edit::Set("units.tower_bolt.projectile", &missile);
    assert!(load(&edited([(UNITS, edit)])).is_ok());
    let wraps = format!(r#"{{ speed = "20", width = "1.6", {steer} }}"#);
    let edit = Edit::Set("units.grasping_wraps.projectile", &wraps);
    let error = load(&edited([(HUSK, edit)])).unwrap_err();
    let steered = |problem: &LoadProblem| {
        rule_at(
            problem,
            &entry("actions", "grasping_wraps"),
            &ProjectileDeliveryError::Steered,
        )
    };
    assert!(steered(&error.problem), "{error:?}");
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
fn a_weapons_splash_needs_a_projectile_its_scatter_a_point_its_speed_none_and_its_pitch_a_bound() {
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
    let steered = (
        "units.tower_bolt.projectile",
        r#"{ speed = "20", steer = { turn_rate = "90", fuel_ms = 3000 } }"#,
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
    // A homing bolt splashes and bounds its pitch; a curve and a steered bolt, which fly to a
    // point, scatter too. A splash of 0 m with an outer ring past it loads on a weapon of its own
    // amount, under a damage bonus or not; beside the stat `damage`, the ring and the bonus are
    // each refused, as no bonus would scale the ring as the weapon's damage.
    assert!(set(&[splash, pitch]).is_ok());
    let ringed = (
        "actions.tower_attack.splash",
        r#"{ radius = "0", affects = "enemies", outer = { radius = "3", amount = "5" } }"#,
    );
    let own = |ring: (&'static str, &'static str), bonus: bool| {
        let mut files = vec![
            (MODE_DATA, Edit::Remove("actions.tower_attack.damage")),
            (
                MODE_DATA,
                Edit::Set("actions.tower_attack.amount", r#""10""#),
            ),
            (MODE_DATA, Edit::Set(ring.0, ring.1)),
        ];
        if bonus {
            files.push((
                MODE_DATA,
                Edit::Set("actions.tower_attack.damage_bonus", r#""armor""#),
            ));
        }
        load(&edited(files))
    };
    assert!(own(ringed, false).is_ok());
    assert!(own(ringed, true).is_ok());
    let refused = |problem: &LoadProblem, rule: &CombatDataError| {
        rule_at(problem, &entry("actions", "tower_attack"), rule)
    };
    let error = set(&[ringed]).unwrap_err();
    assert!(
        refused(&error.problem, &CombatDataError::OuterBesideStat),
        "{error:?}"
    );
    let error = set(&[("actions.tower_attack.damage_bonus", r#""armor""#)]).unwrap_err();
    assert!(
        refused(&error.problem, &CombatDataError::DamageBonus),
        "{error:?}"
    );
    assert!(set(&[splash, scatter, pitch, curve]).is_ok());
    assert!(set(&[scatter, steered]).is_ok());
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
    // An outer ring deals a number at least 0.
    let negative = (
        "actions.tower_attack.splash",
        r#"{ radius = "1", affects = "enemies", outer = { radius = "3", amount = "-5" } }"#,
    );
    let error = own(negative, false).unwrap_err();
    let outer = |problem: &LoadProblem| {
        rule_at(
            problem,
            &entry("actions", "tower_attack"),
            &CombatDataError::OuterAmount,
        )
    };
    assert!(outer(&error.problem), "{error:?}");
    // Each reads only within its bounds.
    let file = "data/mode.toml";
    for (edit, message) in [
        (
            (
                "actions.tower_attack.splash",
                r#"{ radius = "-1", affects = "enemies" }"#,
            ),
            "a splash's radius",
        ),
        (
            (
                "actions.tower_attack.splash",
                r#"{ radius = "3", affects = "enemies", outer = { radius = "3", amount = "5" } }"#,
            ),
            "a splash's outer ring",
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
    // The 3v3's `phase` is sent to all; without `audience`, or with `audience = "none"`, to none.
    let phase = |edit: Edit<'_>| {
        let packages = load(&edited([(MODE_DATA, edit)])).unwrap();
        packages.data().state["phase"].audience
    };
    let pick = r#"phase = { type = "string", default = "pick", audience = "all" }"#;
    assert_eq!(phase(Edit::Replace(pick, pick)), Audience::All);
    let unsent = r#"phase = { type = "string", default = "pick" }"#;
    assert_eq!(phase(Edit::Replace(pick, unsent)), Audience::None);
    let none = r#"phase = { type = "string", default = "pick", audience = "none" }"#;
    assert_eq!(phase(Edit::Replace(pick, none)), Audience::None);
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
    assert_eq!((weapon.on_hit.len(), weapon.keys.params.len()), (1, 1));
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
#[test]
fn the_books_build_at_every_rate_the_manifest_allows() {
    // The 3v3 allows 20 to 60 ticks a second; the load built its books at 60.
    let packages = TestMode::Moba3v3.packages();
    for hz in [20, 30, 60] {
        let rate = MatchRate::new(NonZeroU32::new(hz).unwrap());
        packages.books(rate);
    }
}

#[test]
#[should_panic(expected = "a session's rate is within the manifest's range")]
fn the_books_refuse_a_rate_past_the_manifests_range() {
    let packages = TestMode::Moba3v3.packages();
    let rate = MatchRate::new(NonZeroU32::new(61).unwrap());
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
        "    let near = |unit| unit.position;\n",
        "    let hook = Fn(\"on_resolve\");\n",
    ] {
        let at =
            r#"    for unit in ctx.find(caster, caster.position, ctx.params.radius, "enemies") {"#;
        let edit = Edit::Replace(at, &format!("{line}{at}"));
        assert!(load(&edited([(LASH_OUT, edit)])).is_ok(), "{line}");
    }
}

#[test]
fn a_mode_script_spawns_its_own_unit_types_and_avatars_by_name() {
    // The mode's tower, and Husk by its package's name, are in the mode's scope; the team is one
    // of the manifest's.
    let spawns = r#"    ctx.spawn_unit("tower", "north", unit.position);
    ctx.spawn_unit("hero-husk", "camps", unit.position);
    if unit.has_tag("inhibitor") {
"#;
    let edit = Edit::Replace("    if unit.has_tag(\"inhibitor\") {\n", spawns);
    assert!(load(&edited([(MODE_SCRIPT, edit)])).is_ok());
}

#[test]
fn the_engines_tags_and_the_modes_fill_the_tags_a_match_holds() {
    // The 3v3 names 25 tags: the 10 of its `[tags]`, 6 of its heroes' classes, 8 more of its unit
    // types', `ward` among them, and `slowed`, which modifiers grant. The engine has 12, so 219
    // layers, each a tag, fill the 256 a match holds, and 220 are past it.
    let tags = |packages: &ModePackages| packages.tag_names().len();
    let packages = load(&edited([])).unwrap();
    assert_eq!(tags(&packages), 25);
    // Each once, in the order of their names, as a match declares them: `ward` after `slowed`.
    let tag_names = packages.tag_names();
    let names: Vec<&str> = tag_names.iter().map(String::as_str).collect();
    assert_eq!(
        names,
        [
            "airborne",
            "assassin",
            "camp",
            "core",
            "creep",
            "disarmed",
            "fighter",
            "inhibitor",
            "invulnerable",
            "mage",
            "marksman",
            "objective",
            "rooted",
            "silenced",
            "slow_immune",
            "slowed",
            "stealthed",
            "structure",
            "stunned",
            "support",
            "tank",
            "tower",
            "true_sight",
            "untargetable",
            "ward",
        ]
    );
    let [(path, layers)] = <[_; 1]>::try_from(layers(219)).unwrap();
    let edit = Edit::Set(&path, &layers);
    let packages = load(&edited([(MODE_DATA, edit)])).unwrap();
    assert_eq!(tags(&packages), 244);
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
        // The 3v3's 25 tags and the engine's 12 leave 219 of the 256 a match holds for layers.
        LimitCase {
            more: layers,
            allowed: 219,
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
    let husk = ModePackages::at(
        &PackageDir::new(moba().join("heroes/husk")),
        Some(&two_lanes()),
    )
    .unwrap_err();
    assert!(matches!(*husk.problem, LoadProblem::WrongKind), "{husk}");
}

/// Every ground point of `map` raised to `[x, y, z]`, and the map made spatial: each point of a
/// `position` and a `points` list to y = 0, and a region from y = 0 to 10, as a spatial region is a
/// box with height; not the bounds, which stay on the ground plane.
fn raised(map: &str) -> String {
    fn raise(value: &mut TomlNode, height: Option<i64>) {
        match value {
            TomlNode::Table(table) => {
                let region = table.contains_key("min") && !table.contains_key("cell");
                for (key, inner) in table.iter_mut() {
                    let height = match key.as_str() {
                        "position" | "points" => Some(0),
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
    // placed away from enemies; a gather of gold from a mine, which the yard takes back; and a
    // train of a mob, which stands but walks by the melee creeps it leads, as a train of a tower
    // that stands is refused (04-capabilities/production.md, A unit may lead its spawns): the load
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
                "mode-name = Three on Three\naction-raise = Raise\naction-recruit = Recruit\n",
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
        (
            MODE_DATA,
            Edit::Replace(
                "[actions.melee_creep_attack]",
                "[actions.recruit]\nname = \"action-recruit\"\nkind = \"train\"\ntargeting = \"none\"\nunit_type = \"mob\"\n\n[actions.melee_creep_attack]",
            ),
        ),
        (
            UNITS,
            Edit::Replace(
                "[units.tower]\n",
                "[units.mob]\nspawns = { unit_types = [\"melee_creep\"], count = 2, replace_ms = 1000, led = true }\n\n[units.tower]\n",
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
    let spatial = ModePackages::at(&two, Some(&second)).unwrap();
    assert_eq!(spatial.map_name(), &second);
    assert_eq!(spatial.map().metric(), Metric::Spatial);
    let third = MapName::parse("third").unwrap();
    let error = ModePackages::at(&two, Some(&third)).unwrap_err();
    assert!(matches!(&*error.problem, LoadProblem::NoMap(map) if *map == third));
    // Read once, the mode loads on each of its maps as a load of each reads it again.
    let read = ModeRead::at(&two).unwrap();
    assert_eq!(read.on(Some(&second)).unwrap().map_name(), &second);
    assert_eq!(
        read.on(Some(&two_lanes())).unwrap().map_name(),
        &two_lanes()
    );
    let error = read.on(Some(&third)).unwrap_err();
    assert!(matches!(&*error.problem, LoadProblem::NoMap(map) if *map == third));

    // With no map named, the only map; of two, neither, and the error names both in order.
    let only = ModePackages::at(&edited([]), None).unwrap();
    assert_eq!(only.map_name(), &two_lanes());
    let error = ModePackages::at(&two, None).unwrap_err();
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
fn a_type_of_variations_holds_no_collision_of_its_own_one_to_256_of_them_on_one_layer() {
    // The 3v3's tower with two variations in place of its collision loads; beside its collision,
    // or as none, the variations fail the load at the tower; and on two layers of the mode, so
    // does a variation on the second, as one type moves on one layer.
    let collision = r#"collision = { radius = "0.9" }"#;
    let varied =
        r#"variations = [{ collision = { radius = "0.9" } }, { collision = { radius = "0.5" } }]"#;
    assert!(load(&edited([(UNITS, Edit::Replace(collision, varied))])).is_ok());
    // The map's third unit, a tower, places its second variation; its third fails the load at the
    // map, as the tower has two
    // (04-capabilities/00-overview.md, A placement may give its variation).
    let placed = |place: &'static str| {
        let edits = [
            (UNITS, Edit::Replace(collision, varied)),
            (MAP, Edit::Set("units.2.variation", place)),
        ];
        load(&edited(edits))
    };
    assert!(placed("1").is_ok());
    let past = ModeError::NoVariation {
        unit_type: DeclaredName::new("tower").unwrap(),
        variation: 2,
    };
    let error = placed("2").unwrap_err();
    assert!(rule(&error.problem, &past), "{error:?}");
    let both = format!("{collision}\n{varied}");
    let none = "variations = []";
    let layered = r#"variations = [{ collision = { radius = "0.9" } }, { collision = { radius = "0.5", layer = "air" } }]"#;
    let layers = Edit::Set("navigation.layers", r#"["ground", "air"]"#);
    let cases = [
        (both.as_str(), UnitTypeError::VariedCollision),
        (none, UnitTypeError::VariationCount(0)),
        (layered, UnitTypeError::VariationLayers),
    ];
    for (text, refused) in cases {
        let edits = [(UNITS, Edit::Replace(collision, text)), (MODE_DATA, layers)];
        let error = load(&edited(edits)).unwrap_err();
        let at = rule_at(&error.problem, &unit_type("tower"), &refused);
        assert!(at, "{text}: {error:?}");
    }
}

#[test]
fn a_player_controls_an_avatar_only_in_a_mode_of_avatars() {
    // The 3v3, of heroes, says its players control them alone; the ruled mode, of no avatar,
    // loads with the default control of units, and fails with control of an avatar.
    assert_eq!(
        TestMode::Moba3v3.packages().data().players.control,
        PlayerControl::Avatar
    );
    let ruled = |players| {
        let mut tree = PackageTree::of(TestMode::Ruled);
        tree.edit("modes/ruled/data/mode.toml", Edit::Append(players));
        tree.load()
    };
    let units = ruled("\n[players]\ncontrol = \"units\"\n").unwrap();
    assert_eq!(units.data().players.control, PlayerControl::Units);
    let error = ruled("\n[players]\ncontrol = \"avatar\"\n").unwrap_err();
    assert!(
        rule_at(
            &error.problem,
            &Place::Section(ModeData::PLAYERS),
            &ModeDataError::AvatarControlWithoutAvatars
        ),
        "{error:?}"
    );
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

mod action;
mod data;
mod map;
mod script;
mod stat;
