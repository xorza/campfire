//! The reference 3v3 as its package holds it: it loads, passes design 08's load checks, and
//! plays a match that replays to the same state hashes. And each flaw a package can have fails
//! its load with its own problem.

use std::fs;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use campfire_capabilities::{
    Controller, Hook, InputValue, LaneWalker, ModeError, ModeInput, ModeState, PlayerResources,
    ScriptFailures, StateValue, Team, UnitType,
};
use campfire_content::ContentError;
use campfire_math::{Num, Vec3};
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey};
use campfire_protocol::{
    Delegation, DelegationTerms, InputChain, PlayerSlot, SeedChain, SessionHeader, SessionLog,
    SessionTerms,
};
use campfire_runner::{CtxMisuse, LoadError, LoadProblem, ModePackages, Place, RELEASE, Runner};
use campfire_sim::{Capability, EntityIndex, Position, StableId, StateHash};

/// The 3v3's slowest rate, which runs a match in the fewest ticks.
const TICK_HZ: NonZeroU32 = NonZeroU32::new(20).unwrap();
const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::MIN);
const SERVER_KEY: [u8; 32] = [8; 32];
/// BIP-340 signing without auxiliary randomness is deterministic, so every run signs alike.
const AUX: [u8; 32] = [0; 32];
const PLAYERS: u32 = 6;
const HEROES: [&str; 6] = [
    "hero-cinder",
    "hero-gale",
    "hero-husk",
    "hero-kensho",
    "hero-rime",
    "hero-veil",
];

fn moba() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/moba"))
}

fn packages() -> ModePackages {
    ModePackages::from_dir(&moba().join("modes/3v3")).unwrap_or_else(|error| panic!("{error}"))
}

fn key(byte: u32) -> Keypair {
    let secret = SecretKey::from_byte_array(&[u8::try_from(byte).unwrap(); 32]).unwrap();
    Keypair::from_secret_key(&Secp256k1::new(), &secret)
}

/// Player `slot`'s session key.
fn session_key(slot: u32) -> Keypair {
    key(20 + slot)
}

fn terms(packages: &ModePackages) -> SessionTerms {
    SessionTerms {
        server_key: SERVER_KEY,
        tick_hz: TICK_HZ,
        max_input_delay: 10,
        max_input_lead: 10,
        max_payload_len: 256,
        max_inputs_per_tick: 4,
        seed_commitment: SEED_CHAIN.commitment(),
        release: RELEASE.to_owned(),
        mode: packages.fingerprint(),
        dependencies: packages.dependencies().collect(),
    }
}

/// Player `slot`'s delegation in the session of `terms`.
fn delegation(slot: u32, terms: &SessionTerms) -> Delegation {
    let delegated = DelegationTerms {
        session_key: session_key(slot).x_only_public_key().0,
        server_key: SERVER_KEY,
        session_id: terms.session_id(),
        seed_contribution: [u8::try_from(slot).unwrap(); 32],
        expiration: 1_700_086_400,
    };
    Delegation::sign(
        &Secp256k1::new(),
        &key(10 + slot),
        &delegated,
        1_700_000_000,
        &AUX,
    )
}

fn log(terms: SessionTerms) -> SessionLog {
    let players = (0..PLAYERS).map(|slot| delegation(slot, &terms)).collect();
    SessionLog::new(SessionHeader { terms, players }).unwrap()
}

#[derive(Debug)]
struct Run {
    runner: Runner,
    hashes: Vec<StateHash>,
    /// Each unit after the tick the heroes spawn in, and after the one the first wave spawns in.
    at_pick_end: Vec<Unit>,
    at_first_wave: Vec<Unit>,
    /// The units whose calls failed, of every tick.
    failed: Vec<StableId>,
}

/// A unit as a test sees it: its team, its unit type, where it stands, who controls it, and
/// whether it walks a lane.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Unit {
    team: Team,
    kind: UnitType,
    pos: Position,
    controller: Option<u32>,
    walks: bool,
}

/// The pick lasts 60 s, 1200 ticks, set at the start: it ends in tick 1199. The first wave comes
/// 60 s later, in tick 2399.
const PICK_END: u64 = 1199;
const FIRST_WAVE: u64 = 2399;

fn units(runner: &Runner) -> Vec<Unit> {
    let world = runner.world();
    world
        .resource::<EntityIndex>()
        .iter()
        .filter_map(|(_, entity)| {
            let unit = world.entity(entity);
            Some(Unit {
                team: *unit.get::<Team>()?,
                kind: *unit.get::<UnitType>()?,
                pos: *unit.get::<Position>()?,
                controller: unit.get::<Controller>().map(|controller| controller.slot()),
                walks: unit.contains::<LaneWalker>(),
            })
        })
        .collect()
}

/// A match of `ticks` ticks in which each player picks a hero and two spells before tick 0.
fn run(packages: &ModePackages, ticks: u64) -> Run {
    let terms = terms(packages);
    let mut runner = Runner::new(log(terms.clone()), SEED_CHAIN.seed(0), packages)
        .unwrap_or_else(|error| panic!("{error}"));
    let secp = Secp256k1::new();
    let mut applied = Vec::new();
    for slot in 0..PLAYERS {
        let mut chain =
            InputChain::new(PlayerSlot::new(slot), delegation(slot, &terms).chain_root());
        let payload = ModeInput::payload(&[
            ModeInput {
                name: "hero",
                value: InputValue::String(HEROES[slot as usize]),
            },
            ModeInput {
                name: "spells",
                value: InputValue::StringList(vec!["haste", "mend"]),
            },
        ]);
        let input = chain.extend(0, &payload);
        let signature = chain.sign(&secp, &session_key(slot), terms.session_id(), &AUX);
        runner.record([input], &signature, &mut applied).unwrap();
    }
    let mut run = Run {
        runner,
        hashes: Vec::new(),
        at_pick_end: Vec::new(),
        at_first_wave: Vec::new(),
        failed: Vec::new(),
    };
    for tick in 0..ticks {
        run.runner.run_tick();
        run.hashes.push(run.runner.state_hash());
        let failures = run.runner.world().non_send::<ScriptFailures>();
        for failure in failures.get() {
            assert_eq!(failure.hook, Hook::Think, "{failure:?}");
            run.failed.push(failure.unit.unwrap());
        }
        match tick {
            PICK_END => run.at_pick_end = units(&run.runner),
            FIRST_WAVE => run.at_first_wave = units(&run.runner),
            _ => {}
        }
    }
    run.runner.reveal_seed();
    run
}

fn ground(x: i64, z: i64) -> Position {
    let meters = |value| Num::from_int(value).unwrap();
    Position::new(Vec3::new(meters(x), Num::ZERO, meters(z))).unwrap()
}

#[test]
fn a_3v3_match_replays_to_the_same_hashes() {
    let packages = packages();
    let run = run(&packages, 2500);
    let runner = &run.runner;
    let world = runner.world();
    // State in the order of its fields' names: first_blood, then phase.
    let phase = &world.resource::<ModeState>().get()[1];
    assert_eq!(phase, &StateValue::Text("play".to_owned()));

    // The map's 14 structures from the start; at the pick's end, the 6 heroes at their teams'
    // spawns, slots 0 to 2 north and 3 to 5 south, and the 5 neutral camps.
    let hero = |slot: u32| {
        let team = u8::from(slot >= 3);
        let z = if team == 0 { -60 } else { 60 };
        (Team::new(team), ground(0, z), Some(slot))
    };
    let heroes: Vec<_> = run.at_pick_end[14..20]
        .iter()
        .map(|unit| (unit.team, unit.pos, unit.controller))
        .collect();
    assert_eq!(heroes, (0..PLAYERS).map(hero).collect::<Vec<_>>());
    let camps: Vec<_> = run.at_pick_end[20..]
        .iter()
        .map(|unit| (unit.team, unit.pos))
        .collect();
    let neutral = Team::new(2);
    assert_eq!(
        camps,
        [
            (neutral, ground(-18, -12)),
            (neutral, ground(18, -12)),
            (neutral, ground(-18, 12)),
            (neutral, ground(18, 12)),
            (neutral, ground(0, 0)),
        ]
    );
    // The first wave: on each lane, west then east, each team's six creeps at its end: 24.
    let wave = &run.at_first_wave[25..];
    let seen: Vec<_> = wave.iter().map(|unit| (unit.team, unit.pos)).collect();
    let ends = [(0, -6, -50), (1, -6, 50), (0, 6, -50), (1, 6, 50)];
    let expected: Vec<_> = ends
        .iter()
        .flat_map(|&(team, x, z)| [(Team::new(team), ground(x, z)); 6])
        .collect();
    assert_eq!(seen, expected);
    assert!(wave.iter().all(|unit| unit.walks));
    // Melee creeps first, then casters, both types of the wave list.
    assert_eq!(wave[0].kind, wave[1].kind);
    assert_ne!(wave[0].kind, wave[5].kind);

    // Income: 8 gold every 5 s from the pick's end, 100 ticks, in ticks 1299 to 2499: 13 times.
    let gold = world.resource::<PlayerResources>();
    for slot in 0..PLAYERS {
        assert_eq!(gold.amount(slot, "gold"), 104, "player {slot}");
    }
    // Only camps, 20 to 24, fail: their AI reads `unit.spawn_pos`, which the release does not
    // have yet.
    assert!(!run.failed.is_empty());
    assert!(run.failed.iter().all(|id| (20..25).contains(&id.get())));
    let hashes = &run.hashes;

    let mut file = Vec::new();
    runner.log().encode(&mut file);
    let decoded = SessionLog::decode(&file).unwrap();
    let mut replay = Runner::new(decoded.rewound(), SEED_CHAIN.seed(0), &packages).unwrap();
    for (tick, live) in hashes.iter().enumerate() {
        replay.run_tick();
        assert_eq!(replay.state_hash(), *live, "tick {tick}");
    }
}

/// A copy of every reference package under the target's scratch directory, named `name`, with
/// `edits` made, each to a file by its path from the copy's root; the path of its 3v3.
fn edited<'a>(name: &str, edits: impl IntoIterator<Item = (&'a str, Edit)>) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("mode_package")
        .join(name);
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    copy(&moba(), &root);
    for (file, edit) in edits {
        let path = root.join(file);
        let text = match edit {
            Edit::Replace(from, to) => {
                let text = fs::read_to_string(&path).unwrap();
                assert!(text.contains(from), "{file}: {from}");
                text.replacen(from, to, 1)
            }
            Edit::Create(text) => text.to_owned(),
        };
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    root.join("modes/3v3")
}

/// Text put in place of the first of another, or the text of a new file.
#[derive(Debug, Clone, Copy)]
enum Edit {
    Replace(&'static str, &'static str),
    Create(&'static str),
}

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
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

/// Whether `problem` is the manifest failing to read with a message that starts with `message`.
fn manifest_fails(problem: &LoadProblem, message: &str) -> bool {
    matches!(problem, LoadProblem::Content(ContentError::Data { path, error })
        if path.to_string() == "manifest.toml" && error.message().starts_with(message))
}

#[test]
fn a_caster_creep_projectile_slower_than_the_cap_fails_the_load() {
    // 5 m/s, slower than the 3v3's cap of 6: a homing projectile might never catch a hero.
    let edit = Edit::Replace(r#"projectile_speed = "6.5""#, r#"projectile_speed = "5.0""#);
    let error = ModePackages::from_dir(&edited("slow_caster", [(UNITS, edit)])).unwrap_err();
    assert_eq!(error.package, MODE);
    let at_caster = |problem: &LoadProblem| matches!(problem, LoadProblem::ProjectileNotFaster { at: Place::UnitType(name) } if name == "caster_creep");
    assert!(at_caster(&error.problem), "{error}");
}

/// Each flaw, one to a copy of the packages, and the problem it fails the load with.
const FLAWS: [Flaw; 43] = [
    flaw(
        MANIFEST,
        Edit::Replace(r#"engine = "0.1.0""#, r#"engine = "0.0.9""#),
        MODE,
        |problem| matches!(problem, LoadProblem::OtherEngine(engine) if engine == "0.0.9"),
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
        |problem| matches!(problem, LoadProblem::Undeclared { capability: Capability::Vision, at: Place::UnitType(name) } if name == "tower"),
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
    for (at, flaw) in FLAWS.iter().enumerate() {
        let edits = [(flaw.file, flaw.edit)]
            .into_iter()
            .chain(flaw.also.iter().copied());
        let dir = edited(&format!("flaw_{at}"), edits);
        let error = ModePackages::from_dir(&dir).expect_err(flaw.file);
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
