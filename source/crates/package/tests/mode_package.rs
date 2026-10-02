//! Each flaw a package can have fails the load of the reference packages with its own problem.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use campfire_capabilities::{
    ActionField, ActionKind, EffectData, EffectTo, Effecting, Hook, MapProblem, ModeError, Number,
    PlannedEffect, Scalar,
};
use campfire_package::{
    ChoiceProblem, Content, ContentError, CtxMisuse, DeliveryProblem, EffectProblem, Limit,
    LoadError, LoadProblem, ModePackages, NameKind, PackageDir, Place,
};
use campfire_sim::Capability;

fn moba() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/moba"))
}

/// Every file of the reference packages, by its path from their root, read from disk once.
fn moba_files() -> &'static BTreeMap<PathBuf, Vec<u8>> {
    static FILES: OnceLock<BTreeMap<PathBuf, Vec<u8>>> = OnceLock::new();
    FILES.get_or_init(|| {
        let mut files = BTreeMap::new();
        read_tree(&moba(), Path::new(""), &mut files);
        files
    })
}

fn read_tree(dir: &Path, at: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = at.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            read_tree(&entry.path(), &path, files);
        } else {
            files.insert(path, fs::read(entry.path()).unwrap());
        }
    }
}

/// The reference packages in memory with `edits` made, each to a file by its path from their
/// root: their 3v3.
fn edited<'a>(edits: impl IntoIterator<Item = (&'a str, Edit)>) -> PackageDir {
    let mut files = moba_files().clone();
    for (file, edit) in edits {
        let path = PathBuf::from(file);
        let text = match edit {
            Edit::Replace(from, to) => {
                let text = String::from_utf8(files[&path].clone()).unwrap();
                assert!(text.contains(from), "{file}: {from}");
                text.replacen(from, to, 1)
            }
            Edit::Create(text) => text.to_owned(),
        };
        files.insert(path, text.into_bytes());
    }
    PackageDir::in_memory(Arc::new(files), "modes/3v3")
}

/// Text put in place of the first of another, or the text of a new file.
#[derive(Debug, Clone, Copy)]
enum Edit {
    Replace(&'static str, &'static str),
    Create(&'static str),
}

/// A flaw, the package it is in, and the problem it fails the load with.
#[derive(Debug)]
struct Flaw {
    file: &'static str,
    edit: Edit,
    /// More edits the flaw needs, each to a file.
    also: &'static [(&'static str, Edit)],
    package: &'static str,
    refused: fn(&LoadProblem) -> bool,
}

const fn flaw(
    file: &'static str,
    edit: Edit,
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
const GALE: &str = "heroes/gale/data/avatar.toml";
const CINDER: &str = "heroes/cinder/data/avatar.toml";
const VEIL: &str = "heroes/veil/data/avatar.toml";
const RIME: &str = "heroes/rime/data/avatar.toml";
/// Rime's Fan of Frost's `on_hit` effect that slows.
const SLOWS: &str = r#"{ modifier = { id = "slow", duration_ms = { param = "slow_ms" } } },"#;
const LASH_OUT: &str = "heroes/husk/scripts/lash_out.rhai";
const CREEP_AI: &str = "modes/3v3/scripts/creep_ai.rhai";
const MODE: &str = "moba-3v3";
/// A package whose manifest does not read has no name, so its directory names it.
const MODE_DIR: &str = "modes/3v3";
const MODE_DATA: &str = "modes/3v3/data/mode.toml";
/// A train of a melee creep, which the mode's data does not hold, put before its first action.
const RECRUIT: &str = "[actions.recruit]\nkind = \"train\"\ntargeting = \"none\"\nunit_type = \"melee_creep\"\n\n[actions.melee_creep_attack]";
/// The manifest's capabilities with `production`.
const PRODUCTION: Edit = Edit::Replace(r#""progression"]"#, r#""progression", "production"]"#);

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
    let edit = Edit::Replace(
        r#"projectile = { speed = "6.5", homing = true }"#,
        r#"projectile = { speed = "5.0", homing = true }"#,
    );
    let error = ModePackages::from_package_dir(&edited([(UNITS, edit)])).unwrap_err();
    assert_eq!(error.package, MODE);
    let at_caster = |problem: &LoadProblem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::NotFaster(Place::UnitType(name))) if name == "caster_creep_bolt");
    assert!(at_caster(&error.problem), "{error}");
    // Along a line, the same speed loads: it chases no one.
    let edit = Edit::Replace(
        r#"projectile = { speed = "20", width"#,
        r#"projectile = { speed = "5", width"#,
    );
    assert!(ModePackages::from_package_dir(&edited([(HUSK, edit)])).is_ok());
}

#[test]
fn an_effect_to_the_source_reads_and_any_other_to_does_not() {
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
        .find(|dependent| dependent.package.name == "hero-rime")
        .unwrap();
    let Content::Avatar(avatar) = &rime.content else {
        panic!("Rime is an avatar");
    };
    let heal = EffectData {
        does: Effecting::Heal {
            amount: Number::Value(Scalar::Int(1)),
        },
        to: EffectTo::Source,
    };
    assert_eq!(avatar.actions["fan_of_frost"].on_hit.last(), Some(&heal));
    // `to` names the source alone.
    let to_target = Edit::Replace(SLOWS, r#"{ heal = { amount = 1 }, to = "target" },"#);
    let error = ModePackages::from_package_dir(&edited([(RIME, to_target)])).unwrap_err();
    assert!(
        read_fails(&error.problem, "data/avatar.toml", "unknown variant"),
        "{error}"
    );
}

#[test]
fn more_layers_than_tags_a_match_holds_fail_the_load() {
    // 257 layers, each a tag, past the 256 tags a match holds.
    let names: Vec<String> = (0..=256).map(|at| format!("\"layer{at}\"")).collect();
    let section = format!(
        "[navigation]\nlayers = [{}]\n\n# Its damage kinds",
        names.join(", ")
    );
    let edit = Edit::Replace("# Its damage kinds", section.leak());
    let error = ModePackages::from_package_dir(&edited([(MODE_DATA, edit)])).unwrap_err();
    assert_eq!(error.package, MODE);
    assert!(
        matches!(*error.problem, LoadProblem::TooMany(Limit::Tags)),
        "{error}"
    );
}

#[test]
fn more_tracks_than_a_unit_holds_fail_the_load() {
    // 32 tracks beside `level`, past the 32 a unit holds.
    let mut tracks = String::new();
    for at in 0..32 {
        write!(tracks, "[tracks.skill{at}]\nlevels = [10]\n\n").unwrap();
    }
    let edit = Edit::Replace("[tracks.level]", format!("{tracks}[tracks.level]").leak());
    let error = ModePackages::from_package_dir(&edited([(MODE_DATA, edit)])).unwrap_err();
    assert_eq!(error.package, MODE);
    assert!(
        matches!(*error.problem, LoadProblem::TooMany(Limit::Tracks)),
        "{error}"
    );
}

/// Each flaw, one to a copy of the packages, and the problem it fails the load with.
const FLAWS: [Flaw; 150] = [
    flaw(
        MANIFEST,
        Edit::Replace(r#"engine = "0.1.0""#, r#"engine = "0.0.9""#),
        MODE,
        |problem| matches!(problem, LoadProblem::OtherEngine(engine) if engine.to_string() == "0.0.9"),
    ),
    flaw(
        MANIFEST,
        Edit::Replace(r#"engine = "0.1.0""#, r#"engine = "0.1""#),
        MODE_DIR,
        |problem| manifest_fails(problem, r#""0.1" is not major.minor.patch"#),
    ),
    flaw(
        "heroes/husk/manifest.toml",
        Edit::Replace(r#"engine = "0.1.0""#, r#"engine = "0.0.9""#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::OtherEngine(_)),
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
    flaw(
        MANIFEST,
        Edit::Replace(r#", "vision", "progression"]"#, r#", "progression"]"#),
        MODE,
        |problem| matches!(problem, LoadProblem::Undeclared { capability: Capability::Vision, at: Place::UnitType(name) } if name == "caster_creep"),
    ),
    // Tracks are progression's: positive and ascending, at most one the `level` track, and each a
    // unit type lists one the mode declares.
    flaw(
        MANIFEST,
        Edit::Replace(r#", "progression"]"#, "]"),
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
        Edit::Replace("levels = [280, 660,", "levels = [660, 280,"),
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
    flaw(
        HUSK,
        Edit::Replace(r#"tracks = ["level"]"#, r#"tracks = ["levels"]"#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Track, name, .. } if name == "levels"),
    ),
    flaw(
        MANIFEST,
        Edit::Replace("min = 20", "min = 40"),
        MODE_DIR,
        |problem| manifest_fails(problem, "the tick rate range does not hold its default"),
    ),
    // A player's pool below the whole call of 20 000.
    flaw(
        MANIFEST,
        Edit::Replace("player = 40000", "player = 19999"),
        MODE_DIR,
        |problem| manifest_fails(problem, "a script pool holds less than a whole call"),
    ),
    flaw(
        MANIFEST,
        Edit::Replace("mode = 100000", "mode = 19999"),
        MODE_DIR,
        |problem| manifest_fails(problem, "a script pool holds less than a whole call"),
    ),
    flaw(
        MANIFEST,
        Edit::Replace(r#"max_move_speed = "6.0""#, r#"max_move_speed = "0""#),
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
        Edit::Replace(
            "[16000, 14000, 12000, 10000, 8000]",
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
        |problem| matches!(problem, LoadProblem::Content(_)),
    ),
    // An action delivers a projectile type of its own package, which homes only alone and at a
    // unit, and needs an aim; a weapon's homes. A projectile type is a delivery type alone, a
    // dependency's unit types are all delivery types, and only actions make projectiles.
    flaw(
        HUSK,
        Edit::Replace(
            r#"projectile = { speed = "20", width"#,
            r#"projectile = { speed = "20", homing = true, width"#,
        ),
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
        Edit::Replace(
            "targeting = \"direction\"\nrange = \"11.0\"",
            "targeting = \"none\"\nrange = \"11.0\"",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::NoAim(action)) if action == "grasping_wraps"),
    ),
    flaw(
        HUSK,
        Edit::Replace(
            "[units.grasping_wraps]\n",
            "[units.husk_dummy]\ntags = [\"dummy\"]\n\n[units.grasping_wraps]\n",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::NotDelivery(Place::UnitType(name))) if name == "hero-husk/husk_dummy"),
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
        Edit::Replace(
            r#"projectile = { speed = "12", homing = true }"#,
            r#"projectile = { speed = "12" }"#,
        ),
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
        Edit::Replace(
            "targeting = \"point\"\nrange = \"9.0\"",
            "targeting = \"direction\"\nrange = \"9.0\"",
        ),
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
        Edit::Replace(
            r#"delay_ms = 625, affects = "enemies" }"#,
            r#"delay_ms = 625, affects = "enemies:molten" }"#,
        ),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Filter, at: Place::UnitType(name), .. } if name == "hero-cinder/eruption"),
    ),
    flaw(
        CINDER,
        Edit::Replace(
            "[units.eruption]\n",
            "[units.eruption]\nprojectile = { speed = \"20\" }\n",
        ),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Delivery(DeliveryProblem::NotDelivery(Place::UnitType(name))) if name == "hero-cinder/eruption"),
    ),
    flaw(
        VEIL,
        Edit::Replace(r#"self = "smoke_ring_cover""#, r#"self = "smoke_cover""#),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Modifier, at: Place::UnitType(unit_type), name } if unit_type == "hero-veil/smoke_ring" && name == "smoke_cover"),
    ),
    flaw(
        MANIFEST,
        Edit::Replace(r#""areas", "#, ""),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Undeclared { capability: Capability::Areas, at: Place::UnitType(name) } if name == "hero-cinder/eruption"),
    ),
    flaw(
        MAP,
        Edit::Replace("unit_type = \"tower\"", "unit_type = \"tower_bolt\""),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::UnknownUnitType(name)) if name == "tower_bolt"),
    ),
    flaw(
        UNITS,
        Edit::Replace(
            "[units.tower_bolt]\n",
            "[units.\"hero-husk/grasping_wraps\"]\nprojectile = { speed = \"20\" }\n\n[units.tower_bolt]\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::RepeatedUnitType(name) if name == "hero-husk/grasping_wraps"),
    ),
    flaw(
        RIME,
        Edit::Replace(SLOWS, r#"{ purge = { tag = "slowed" } },"#),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Effect { action, list: Hook::OnHit, problem: EffectProblem::Planned(PlannedEffect::Purge) } if action == "fan_of_frost"),
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
        Edit::Replace(
            "delivery = { projectile = \"frost_arrow\", count = 7, spread_deg = \"57.5\" }\n",
            "",
        ),
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
        Edit::Replace(
            "base = [40, 50, 60, 70, 80]",
            "base = [40, 50, -60, 70, 80]",
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
        Edit::Replace(
            "slow_ms = 2000\n\n[actions.snow_owl]",
            "slow_ms = \"2000.5\"\n\n[actions.snow_owl]",
        ),
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
        HUSK,
        Edit::Replace("[units.grasping_wraps]", "[units.\"x/grasping_wraps\"]"),
        MODE,
        |problem| matches!(problem, LoadProblem::Slash(name) if name == "x/grasping_wraps"),
    ),
    Flaw {
        file: MANIFEST,
        edit: Edit::Replace("hero-husk = {", "\"hero/husk\" = {"),
        also: &[(
            "heroes/husk/manifest.toml",
            Edit::Replace("name = \"hero-husk\"", "name = \"hero/husk\""),
        )],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Slash(name) if name == "hero/husk"),
    },
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
        |problem| matches!(problem, LoadProblem::UnreferencedScript(path) if path.to_string() == "scripts/extra.rhai"),
    ),
    flaw(
        HUSK,
        Edit::Replace("scripts/dread.rhai", "scripts/dreadful.rhai"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::MissingScript(path) if path.to_string() == "scripts/dreadful.rhai"),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace("fn on_damage_taken(", "fn on_damage_takn("),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::UnknownHook { function, .. } if function == "on_damage_takn"),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace("fn on_damage_taken(", "fn calc_damage_taken("),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::UnknownHook { function, .. } if function == "calc_damage_taken"),
    ),
    flaw(
        CREEP_AI,
        Edit::Replace(
            "fn on_think(ctx, unit) {",
            "fn on_resolve(ctx, caster, target) {}\n\nfn on_think(ctx, unit) {",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::UnknownHook { function, .. } if function == "on_resolve"),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace("fn on_resolve(", "fn on_cast("),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::UnknownHook { function, .. } if function == "on_cast"),
    ),
    flaw(
        CREEP_AI,
        Edit::Replace("fn on_think(ctx, unit)", "fn on_think(ctx, unit, more)"),
        MODE,
        |problem| matches!(problem, LoadProblem::UnknownHook { function, .. } if function == "on_think"),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace("    for unit in", "    let c = ctx;\n    for unit in"),
        "hero-husk",
        |problem| {
            matches!(
                problem,
                LoadProblem::CtxMisuse {
                    misuse: CtxMisuse::Stray,
                    ..
                }
            )
        },
    ),
    flaw(
        CREEP_AI,
        Edit::Replace("fn defend_hero(ctx, unit)", "fn defend_hero(c, unit)"),
        MODE,
        |problem| matches!(problem, LoadProblem::CtxMisuse { misuse: CtxMisuse::Renamed { function }, .. } if function == "defend_hero"),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace(
            "fn on_damage_taken(ctx, m, d)",
            "fn on_damage_taken(m, ctx, d)",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::CtxMisuse { misuse: CtxMisuse::HookParam { function }, .. } if function == "on_damage_taken"),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace("    for unit in", "    let ctx = 1;\n    for unit in"),
        "hero-husk",
        |problem| {
            matches!(
                problem,
                LoadProblem::CtxMisuse {
                    misuse: CtxMisuse::Bound,
                    ..
                }
            )
        },
    ),
    flaw(
        LASH_OUT,
        Edit::Replace("caster.pos", "caster.position"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::UnknownMember { name, .. } if name == "position"),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace(
            "    for unit in",
            "    ctx.spawn_avatars();\n    for unit in",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::UnknownCtx { name, .. } if name == "spawn_avatars"),
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
        |problem| matches!(problem, LoadProblem::KindField(action) if action == "melee_creep_attack"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("range = \"7.75\"", "range = \"global\""),
        MODE,
        |problem| matches!(problem, LoadProblem::KindField(action) if action == "tower_attack"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            "[actions.wolf_attack]\n",
            "[actions.wolf_attack]\ncooldown_ms = 1000\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::KindField(action) if action == "wolf_attack"),
    ),
    flaw(
        HUSK,
        Edit::Replace(
            "[actions.dread]\n",
            "[actions.dread]\nrate = \"attack_speed\"\n",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::KindField(action) if action == "dread"),
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
    flaw(
        MAP,
        Edit::Replace("[grid]\ncell = \"1.0\"\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::NoGrid),
    ),
    flaw(
        MAP,
        Edit::Replace("[navigation]\ncell = \"0.5\"\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::NoPathingGrid),
    ),
    flaw(
        MAP,
        Edit::Replace("cell = \"1.0\"", "cell = \"0\""),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::Grid)),
    ),
    flaw(
        MAP,
        Edit::Replace("min = [-48, -68]", "min = [48, -68]"),
        MODE,
        |problem| read_fails(problem, "map/map.toml", "bounds need min below max"),
    ),
    flaw(
        MAP,
        Edit::Replace("pos = [0, -54]", "pos = [0, -69]"),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::OutOfBounds)),
    ),
    // A meter from the west lane's waypoint 1, at (−36, −36), less than the tower's body and the
    // widest walker's together.
    flaw(
        MAP,
        Edit::Replace("pos = [-22, -46]", "pos = [-36, -37]"),
        MODE,
        |problem| {
            matches!(problem, LoadProblem::Map(MapProblem::WaypointBlocked { path, waypoint })
                if path == "west" && *waypoint == 1)
        },
    ),
    // A meter from the north spawn, at (0, −60).
    flaw(
        MAP,
        Edit::Replace("pos = [-22, -46]", "pos = [0, -59]"),
        MODE,
        |problem| {
            matches!(problem, LoadProblem::Map(MapProblem::MarkerBlocked { marker })
                if marker == "north_spawn")
        },
    ),
    flaw(
        UNITS,
        Edit::Replace(r#"sight_range = "10.0""#, r#"sight_range = "-1.0""#),
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
        Edit::Replace("cost = { mana = 35 }", "cost = { mana = -35 }"),
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
    flaw(
        UNITS,
        Edit::Replace(
            "[units.melee_creep]",
            "[units.hero-husk]\n\n[units.melee_creep]",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::RepeatedUnitType(name) if name == "hero-husk"),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace("ctx.find(", "ctx.finds("),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::UnknownCtx { name, .. } if name == "finds"),
    ),
    // An AI's order in the mode's script.
    flaw(
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(
            "fn on_match_start(ctx) {",
            "fn on_match_start(ctx) {\n    ctx.order_reset(());",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::UnknownCtx { name, .. } if name == "order_reset"),
    ),
    flaw(
        LASH_OUT,
        Edit::Replace("ctx.p.radius", "ctx.p.radios"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Param, at: Place::Script(path), name } if path.to_string() == "scripts/lash_out.rhai" && name == "radios"),
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
        Edit::Replace("magic_resist = { base = 30 }", "spirit = { base = 30 }"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Stat, name, .. } if name == "spirit"),
    ),
    flaw(
        HUSK,
        Edit::Replace("stats = { magic_resist = -15 }", "stats = { spirit = -15 }"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Stat, name, .. } if name == "spirit"),
    ),
    // A stat change names one operation; a scaling param scales with declared stats alone.
    flaw(
        "spells/data/loadout.toml",
        Edit::Replace(
            r#"move_speed = { pct = "0.27" }"#,
            r#"move_speed = { pct = "0.27", cut = "0.1" }"#,
        ),
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
        Edit::Replace(r#"ability_power = "0.7""#, r#"spell_power = "0.7""#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Stat, at: Place::Action(_), name } if name == "spell_power"),
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
        |problem| matches!(problem, LoadProblem::RepeatedPool { name, .. } if name.as_str() == "mana"),
    ),
    flaw(
        HUSK,
        Edit::Replace(r#"pools = ["health", "mana"]"#, r#"pools = ["mana"]"#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::LifePoolMissing(Place::Avatar(_))),
    ),
    flaw(
        UNITS,
        Edit::Replace("pools = [\"health\"]\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::LifePoolMissing(Place::UnitType(name)) if name == "melee_creep"),
    ),
    flaw(
        UNITS,
        Edit::Replace("combat = { on_death = \"stay\" }\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::CombatMissing(Place::UnitType(name)) if name == "inhibitor"),
    ),
    flaw(
        HUSK,
        Edit::Replace("cost = { mana = 35 }", "cost = { rage = 35 }"),
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
        |problem| matches!(problem, LoadProblem::RepeatedName(name) if name.as_str() == "gold"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(r#"resources = ["gold"]"#, r#"resources = ["gold", "mana"]"#),
        MODE,
        |problem| matches!(problem, LoadProblem::RepeatedName(name) if name.as_str() == "mana"),
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
        Edit::Replace(
            r#"cooldown_reduction = { min = 0, max = "0.4" }"#,
            r#"cooldown_reduction = { min = 1, max = "0.4" }"#,
        ),
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
        |problem| matches!(problem, LoadProblem::Content(_)),
    ),
    flaw(
        "modes/3v3/data/mode.toml",
        Edit::Replace(r#"default = "pick", sync = "all""#, r#"default = "pick""#),
        MODE,
        |problem| matches!(problem, LoadProblem::StateSync(field) if field == "phase"),
    ),
    flaw(
        HUSK,
        Edit::Replace(r#""dread", "lash_out""#, r#""grasping_wraps", "lash_out""#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::RepeatedSlot(id) if id == "grasping_wraps"),
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
        Edit::Replace("pos = [0, -60]", "pos = [0, 0, -60]"),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::PointShape)),
    ),
    flaw(
        MAP,
        Edit::Replace(
            "pos = [-18, -12]",
            "region = { min = [-20, -14], max = [-16, -70] }",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::Region(marker)) if marker == "camp1"),
    ),
    flaw(
        MAP,
        Edit::Replace(
            "pos = [-18, -12]",
            "pos = [-18, -12]\nregion = { min = [-20, -14], max = [-16, -10] }",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::Region(marker)) if marker == "camp1"),
    ),
    flaw(
        MAP,
        Edit::Replace(
            "path = \"west\"\npos = [-22, -46]",
            "path = \"middle\"\npos = [-22, -46]",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::UnknownPath(path)) if path == "middle"),
    ),
    flaw(
        MAP,
        Edit::Replace(
            "unit_type = \"core\"\nteam = \"north\"\npos = [0, -54]",
            "unit_type = \"core\"\nteam = \"north\"\npos = [0, -54]\nfrom = \"start\"",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::NoPathToWalk(unit_type)) if unit_type == "core"),
    ),
    flaw(
        MAP,
        Edit::Replace(r#"name = "camp2""#, r#"name = "camp1""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::RepeatedName(name)) if name == "camp1"),
    ),
    // The layers have names of their own, a unit type moves on one of them, and they are
    // navigation's.
    flaw(
        MODE_DATA,
        Edit::Replace(
            "# Its damage kinds",
            "[navigation]\nlayers = [\"ground\", \"ground\"]\n\n# Its damage kinds",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::RepeatedName(name) if name.as_str() == "ground"),
    ),
    flaw(
        UNITS,
        Edit::Replace(
            r#"collision = { radius = "0.35" }"#,
            r#"collision = { radius = "0.35", layer = "air" }"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Layer, at: Place::UnitType(unit_type), name: layer } if unit_type == "melee_creep" && layer.as_str() == "air"),
    ),
    Flaw {
        file: MANIFEST,
        edit: Edit::Replace(r#""orders", "navigation", "vision""#, r#""vision""#),
        also: &[(
            MODE_DATA,
            Edit::Replace(
                "# Its damage kinds",
                "[navigation]\nlayers = [\"ground\"]\n\n# Its damage kinds",
            ),
        )],
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
        |problem| matches!(problem, LoadProblem::Choice(ChoiceProblem::UnknownSlotKind { at: Place::Avatar(name), kind }) if name == "Husk" && kind == "ultimates"),
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
        |problem| matches!(problem, LoadProblem::RepeatedName(name) if name.as_str() == "basic"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace("count = 2\nslot = \"spell\"", "count = 2"),
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
        refused: |problem| matches!(problem, LoadProblem::KindField(action) if action == "recruit"),
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
        |problem| matches!(problem, LoadProblem::KindField(action) if action == "dread"),
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
            "[modifiers.warden_blessing]\naffects = \"allies:ward\"\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Filter, name: filter, .. } if filter == "allies:ward"),
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
        Edit::Replace(r#"targeting = "enemies""#, r#"targeting = "enemies:ward""#),
        "hero-kensho",
        |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::Filter, name: filter, .. } if filter == "enemies:ward"),
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
                    "name = \"more-spells\"\nversion = \"0.1.0\"\nengine = \"0.1.0\"\nkind = \"loadout\"\n",
                ),
            ),
            (
                "more/data/loadout.toml",
                Edit::Create("[actions.haste]\ntargeting = \"none\"\n"),
            ),
        ],
        package: "player-spells",
        refused: |problem| matches!(problem, LoadProblem::RepeatedLoadout(id) if id == "haste"),
    },
];

#[test]
fn every_flaw_of_a_package_fails_its_load_with_its_own_problem() {
    for flaw in &FLAWS {
        let edits = [(flaw.file, flaw.edit)]
            .into_iter()
            .chain(flaw.also.iter().copied());
        let error = ModePackages::from_package_dir(&edited(edits)).expect_err(flaw.file);
        let LoadError { package, problem } = &error;
        assert!(
            Path::new(package).ends_with(flaw.package) && (flaw.refused)(problem),
            "{flaw:?}: {error}"
        );
    }

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
        "{error}"
    );

    // A hero is no mode.
    let husk = ModePackages::from_dir(&moba().join("heroes/husk")).unwrap_err();
    assert!(matches!(*husk.problem, LoadProblem::WrongKind), "{husk}");
}
