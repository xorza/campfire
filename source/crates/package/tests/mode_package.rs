//! Each flaw a package can have fails the load of the reference packages with its own problem.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use campfire_capabilities::{AbilityField, ModeError};
use campfire_package::{
    ContentError, CtxMisuse, LoadError, LoadProblem, ModePackages, PackageDir, Place,
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
const HUSK: &str = "heroes/husk/data/hero.toml";
const GALE: &str = "heroes/gale/data/hero.toml";
const LASH_OUT: &str = "heroes/husk/scripts/lash_out.rhai";
const CREEP_AI: &str = "modes/3v3/scripts/creep_ai.rhai";
const MODE: &str = "moba-3v3";
/// A package whose manifest does not read has no name, so its directory names it.
const MODE_DIR: &str = "modes/3v3";
const MODE_DATA: &str = "modes/3v3/data/mode.toml";

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
    let edit = Edit::Replace(r#"projectile_speed = "6.5""#, r#"projectile_speed = "5.0""#);
    let error = ModePackages::from_package_dir(&edited([(UNITS, edit)])).unwrap_err();
    assert_eq!(error.package, MODE);
    let at_caster = |problem: &LoadProblem| matches!(problem, LoadProblem::ProjectileNotFaster { at: Place::UnitType(name) } if name == "caster_creep");
    assert!(at_caster(&error.problem), "{error}");
}

/// Each flaw, one to a copy of the packages, and the problem it fails the load with.
const FLAWS: [Flaw; 54] = [
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
        Edit::Replace(r#", "vision"]"#, "]"),
        MODE,
        |problem| matches!(problem, LoadProblem::Undeclared { capability: Capability::Vision, at: Place::UnitType(name) } if name == "caster_creep"),
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
        |problem| matches!(problem, LoadProblem::RankCount { ability, ranks: 5 } if ability == "grasping_wraps"),
    ),
    flaw(
        HUSK,
        Edit::Replace("[combat.attack]", "[attack]"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Content(_)),
    ),
    flaw(
        HUSK,
        Edit::Replace(
            r#"projectile = { speed = "20""#,
            r#"projectile = { speed = "5""#,
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::ProjectileNotFaster { at: Place::Ability(id) } if id == "grasping_wraps"),
    ),
    flaw(
        "heroes/husk/scripts/extra.rhai",
        Edit::Create("fn on_cast(ctx, caster, target) {}"),
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
        CREEP_AI,
        Edit::Replace(
            "fn think(ctx, unit) {",
            "fn on_cast(ctx, caster, target) {}\n\nfn think(ctx, unit) {",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::UnknownHook { function, .. } if function == "on_cast"),
    ),
    flaw(
        CREEP_AI,
        Edit::Replace("fn think(ctx, unit)", "fn think(ctx, unit, more)"),
        MODE,
        |problem| matches!(problem, LoadProblem::UnknownHook { function, .. } if function == "think"),
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
        MAP,
        Edit::Replace(
            "[grid]\ncell = \"1.0\"\nmin = [-48, -68]\nmax = [48, 68]\n",
            "",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::NoGrid),
    ),
    flaw(
        MAP,
        Edit::Replace("cell = \"1.0\"", "cell = \"0\""),
        MODE,
        |problem| read_fails(problem, "map/map.toml", "a grid needs a positive cell"),
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
        Edit::Replace("cost = 35", "cost = -35"),
        "hero-husk",
        |problem| {
            matches!(
                problem,
                LoadProblem::AbilityField {
                    field: AbilityField::Cost,
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
        |problem| matches!(problem, LoadProblem::UnknownParam { at: Place::Script(path), name } if path.to_string() == "scripts/lash_out.rhai" && name == "radios"),
    ),
    flaw(
        "heroes/cinder/scripts/wildfire.rhai",
        Edit::Replace(r#""kindle""#, r#""kindl""#),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::UnknownModifier { id, .. } if id == "kindl"),
    ),
    flaw(
        "heroes/veil/scripts/dual_path.rhai",
        Edit::Replace(r#"stat("attack_damage")"#, r#"stat("attack_dmg")"#),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::UnknownStat { name, .. } if name == "attack_dmg"),
    ),
    flaw(
        CREEP_AI,
        Edit::Replace("enemies:creep", "enemies:minion"),
        MODE,
        |problem| matches!(problem, LoadProblem::UnknownFilter { filter, .. } if filter == "enemies:minion"),
    ),
    flaw(
        HUSK,
        Edit::Replace("magic_resist = { base = 30 }", "spirit = { base = 30 }"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::UnknownStat { name, .. } if name == "spirit"),
    ),
    flaw(
        HUSK,
        Edit::Replace("stats = { magic_resist = -15 }", "stats = { spirit = -15 }"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::UnknownStat { name, .. } if name == "spirit"),
    ),
    flaw(
        HUSK,
        Edit::Replace(r#"resource = "mana""#, r#"resource = "rage""#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::UnknownResource { name, .. } if name.as_str() == "rage"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            r#"resources = ["mana", "energy"]"#,
            r#"resources = ["mana", "mana"]"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::RepeatedName(name) if name.as_str() == "mana"),
    ),
    flaw(
        MODE_DATA,
        Edit::Replace(
            r#""slow", "spell_vamp","#,
            r#""slow", "spell_vamp", "move_speed","#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::EngineStatDeclared(name) if name.as_str() == "move_speed"),
    ),
    flaw(
        "heroes/veil/scripts/whirling_blades.rhai",
        Edit::Replace(r#""physical""#, r#""fire""#),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::UnknownDamageKind { kind, .. } if kind == "fire"),
    ),
    flaw(
        GALE,
        Edit::Replace(
            r#"{ param = "bonus_speed" }"#,
            r#"{ param = "bonus_speeds" }"#,
        ),
        "hero-gale",
        |problem| matches!(problem, LoadProblem::UnknownParam { at: Place::Modifier(id), name } if id == "gust_speed" && name == "bonus_speeds"),
    ),
    flaw(
        "heroes/kensho/data/hero.toml",
        Edit::Replace(r#"hold = "still_mind""#, r#"hold = "still_mindful""#),
        "hero-kensho",
        |problem| matches!(problem, LoadProblem::UnknownModifier { at: Place::Ability(ability), id } if ability == "still_mind" && id == "still_mindful"),
    ),
    flaw(
        GALE,
        Edit::Replace(r#"affects = "allies""#, r#"affects = "allies:friends""#),
        "hero-gale",
        |problem| matches!(problem, LoadProblem::UnknownFilter { filter, .. } if filter == "allies:friends"),
    ),
    flaw(
        "heroes/rime/data/hero.toml",
        Edit::Replace(r#"hits = "enemies:hero""#, r#"hits = "foes:hero""#),
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
        Edit::Replace(r#"lane = "west""#, r#"lane = "north""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::UnknownLane(name)) if name == "north"),
    ),
    flaw(
        MAP,
        Edit::Replace("north = [0, -60]\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::NoSpawn(team)) if team == "north"),
    ),
    flaw(
        MANIFEST,
        Edit::Replace(r#"name = "south""#, r#"name = "neutral""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Mode(ModeError::NeutralTeam)),
    ),
    flaw(
        "heroes/kensho/data/hero.toml",
        Edit::Replace(r#"targeting = "enemies""#, r#"targeting = "enemies:ward""#),
        "hero-kensho",
        |problem| matches!(problem, LoadProblem::UnknownFilter { filter, .. } if filter == "enemies:ward"),
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
                    "name = \"more-spells\"\nversion = \"0.1.0\"\nengine = \"0.1.0\"\nkind = \"spells\"\n",
                ),
            ),
            (
                "more/data/spells.toml",
                Edit::Create("[abilities.haste]\ntargeting = \"none\"\n"),
            ),
        ],
        package: "player-spells",
        refused: |problem| matches!(problem, LoadProblem::RepeatedSpell(id) if id == "haste"),
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

    // A hero is no mode.
    let husk = ModePackages::from_dir(&moba().join("heroes/husk")).unwrap_err();
    assert!(matches!(*husk.problem, LoadProblem::WrongKind), "{husk}");
}
