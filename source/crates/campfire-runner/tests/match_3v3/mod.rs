//! The reference 3v3 as its packages hold it, played by scripted players, plays a match with no
//! failed call that replays to the same state hashes: a skirmish of first blood, mend, haste and
//! a tower that turns on a diver; a camp whose wolf answers, resets past its leash and falls; the
//! lanes, where heroes farm the first waves; heroes that learn ranks with their points; then the
//! farm to level 6, the shop, and a showcase in which each hero uses its items and casts each of
//! its abilities. Each rule's result is computed from the mode's numbers. The test pins the
//! content's units and values, so a change to the content changes it, by design. It plays a
//! match long enough for every hero to learn its ultimate, and takes several seconds.

mod showcase;

use bevy_ecs::world::World;
use campfire_capabilities::internals;
use campfire_capabilities::{
    ActionSlot, ActionSlots, Dead, Deaths, Level, ModeParam, ModeState, Owner, PathWalker, Points,
    Scalar, ScriptFailures, SlotKind, StateValue, Stats, Team, UnitType,
};
use campfire_common::Tick;
use campfire_math::{Num, Vec3};
use campfire_protocol::SessionLog;
use campfire_runner::Runner;
use campfire_runner::internals::{FixedMatch, Golden, HashTrail, MatchUnits, Reference3v3};
use campfire_script::ScriptHost;
use campfire_sim::{EntityIndex, Position, StableId, TickRate};

use crate::match_3v3::showcase::Showcase;
use crate::reference::{gold, level_xp, life, respawn_at};

/// The ticks of the skirmish: the pick, the skirmish, the camp, and the first 400 ticks of the
/// first waves' fight, which begins in tick 2803.
const SKIRMISH: u64 = 3200;
/// The ticks the match plays: the skirmish, the farm, the shop, and the showcase's casts, with
/// the window of its last.
const TICKS: u64 = Reference3v3::SHOWCASE + Reference3v3::CAST_EVERY * Reference3v3::CASTS;

#[derive(Debug)]
struct Run {
    fixed: FixedMatch,
    trail: HashTrail,
    golden: Golden,
    /// Each unit after the tick the heroes spawn in, and after the one the first wave spawns in.
    at_pick_end: Vec<Unit>,
    at_first_wave: Vec<Unit>,
    deaths: Vec<Death>,
    seen: Seen,
    showcase: Showcase,
}

/// A unit that died: in tick `tick`, felled by `killer` with `assisters`, and the tick it comes
/// back in, if its type stays.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Death {
    tick: u64,
    unit: StableId,
    killer: Option<StableId>,
    assisters: Vec<StableId>,
    respawn: Option<Tick>,
}

/// What the run reads in the ticks its rules play out in.
#[derive(Debug, Default)]
struct Seen {
    /// Whether Husk holds haste after ticks 1400 and 1401.
    hasted: Vec<bool>,
    /// Gale's life after ticks 1878 to 1880.
    gale: Vec<Num>,
    /// After each of ticks 2190 to 2219, the target of the south's west outer tower, and
    /// Kensho's life.
    tower: Vec<(Option<StableId>, Num)>,
    /// Each hero's experience after tick 2312, when the skirmish's deaths have shared theirs.
    xp_after_skirmish: Vec<Num>,
    /// The south camp's wolf's target after tick 2460; how far from its camp it chases, in
    /// ticks 2500 to 2619; and its target and its place after tick 2640.
    wolf_answers: Option<StableId>,
    wolf_farthest: Num,
    wolf_home: Option<(Option<StableId>, Position)>,
    /// Each hero's experience after ticks 2993 and 2994, the wolf's fall.
    xp_at_wolf_fall: Vec<Vec<Num>>,
    /// Cinder's, Veil's and Rime's learning after ticks 1200, 1201 and 1900, in that order.
    learning: Vec<[Learning; 3]>,
    /// Each player's gold at the skirmish's end.
    gold: Vec<i64>,
}

/// A hero's learning as a tick left it: the ranks of its basic abilities and its ultimate, its
/// unspent points, and whether it is dead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Learning {
    ranks: [u8; 4],
    points: u32,
    dead: bool,
}

impl Learning {
    fn of(world: &World, hero: StableId) -> Learning {
        let entity = world.resource::<EntityIndex>().get(hero).unwrap();
        let unit = world.entity(entity);
        let slots = unit.get::<ActionSlots>().unwrap();
        Learning {
            ranks: [0, 1, 2, 3].map(|slot| slots.slot(slot).unwrap().rank),
            points: unit.get::<Points>().unwrap().get(),
            dead: unit.contains::<Dead>(),
        }
    }
}

/// A unit as a test sees it: its stable id, its team, its unit type, where it stands, who
/// controls it, whether it walks a lane, and its ability slots.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Unit {
    id: StableId,
    team: Team,
    kind: UnitType,
    pos: Position,
    controller: Option<u32>,
    walks: bool,
    slots: Vec<ActionSlot>,
}

/// Player `slot`'s hero, as the pick's end spawned it.
fn hero_of(run: &Run, slot: u32) -> StableId {
    let hero = run
        .at_pick_end
        .iter()
        .find(|unit| unit.controller == Some(slot));
    hero.expect("each player has a hero").id
}

/// The mode param `name` of the reference, a number of milliseconds.
fn param_ms(reference: &Reference3v3, name: &str) -> u64 {
    let ModeParam::Value(Scalar::Int(ms)) = reference.packages().data().params[name] else {
        panic!("{name} is a whole number");
    };
    u64::try_from(ms).unwrap()
}

fn units(runner: &Runner) -> Vec<Unit> {
    let world = runner.world();
    world
        .resource::<EntityIndex>()
        .iter()
        .filter_map(|(id, entity)| {
            let unit = world.entity(entity);
            Some(Unit {
                id,
                team: *unit.get::<Team>()?,
                kind: *unit.get::<UnitType>()?,
                pos: *unit.get::<Position>()?,
                controller: unit.get::<Owner>().map(|owner| owner.slot().get()),
                walks: unit.contains::<PathWalker>(),
                slots: unit
                    .get::<ActionSlots>()
                    .map_or_else(Vec::new, |slots| slots.iter().collect()),
            })
        })
        .collect()
}

/// A match of `TICKS` ticks in which each player picks a hero and two spells before tick 0, and
/// then plays as the reference's script says.
fn run(reference: &Reference3v3) -> Run {
    let mut fixed = reference.start();
    let runner = fixed.runner();
    // A timer fires in the tick its time ends in. The pick's, set in tick 0, ends in the tick
    // before its count of ticks; the first wave's, set then, its own count later.
    let rate = *runner.world().resource::<TickRate>();
    let timer = |name| rate.ticks(param_ms(reference, name)).unwrap().get();
    let pick_end = timer("pick_ms") - 1;
    let first_wave = pick_end + timer("first_wave_ms");
    let mut trail = HashTrail::default();
    let mut golden = Golden::new(reference.packages(), Reference3v3::PLAYERS);
    let (mut at_pick_end, mut at_first_wave) = (Vec::new(), Vec::new());
    let mut deaths = Vec::new();
    let mut seen = Seen::default();
    let mut showcase = Showcase::default();
    for tick in 0..TICKS {
        reference.play_tick(&mut fixed, tick);
        let runner = fixed.runner();
        trail.record(runner.world());
        golden.record_hashed(runner, trail.last());
        let world = runner.world();
        let failures = world.non_send::<ScriptFailures>();
        assert!(
            failures.get().is_empty(),
            "tick {tick}: {:?}",
            failures.get()
        );
        deaths.extend(world.resource::<Deaths>().iter().map(|death| Death {
            tick,
            unit: death.fallen.unit,
            killer: death.killer,
            assisters: death.assisters.to_vec(),
            respawn: respawn_at(world, death.fallen.unit),
        }));
        if tick == pick_end {
            at_pick_end = units(runner);
        } else if tick == first_wave {
            at_first_wave = units(runner);
        }
        if (pick_end..SKIRMISH).contains(&tick) {
            seen.read(world, reference, tick);
        }
        showcase.read(world, reference, tick);
    }
    fixed.runner_mut().reveal_seed();
    Run {
        fixed,
        trail,
        golden,
        at_pick_end,
        at_first_wave,
        deaths,
        seen,
        showcase,
    }
}

impl Seen {
    /// Reads what tick `tick` of the 3v3 of `reference`, which `world` just ran, shows of the
    /// rules the script plays out.
    fn read(&mut self, world: &World, reference: &Reference3v3, tick: u64) {
        let units = MatchUnits::of_world(world);
        let heroes = [0, 1, 2, 3, 4, 5].map(|slot| units.hero(slot));
        let [_, gale, husk, kensho, ..] = heroes;
        let target = |unit| {
            let entity = world.resource::<EntityIndex>().get(unit).unwrap();
            world.get::<ActionSlots>(entity).unwrap().attack_target()
        };
        let xp = || heroes.map(|hero| level_xp(world, reference, hero)).to_vec();
        if [1400, 1401].contains(&tick) {
            let spells = reference
                .packages()
                .packages()
                .position(|view| view.package.header.name == "player-spells");
            let spells = u16::try_from(spells.unwrap()).unwrap();
            let haste = Stats::modifier(world, spells, "haste").unwrap();
            let carried = internals::carried(world, husk);
            self.hasted.push(carried.iter().any(|&(id, _)| id == haste));
        }
        if (1878..=1880).contains(&tick) {
            self.gale.push(life(world, reference, gale));
        }
        if (2190..2220).contains(&tick) {
            let tower = units.spawned_at(-42, 16);
            self.tower
                .push((target(tower), life(world, reference, kensho)));
        }
        if tick == 2312 {
            self.xp_after_skirmish = xp();
        }
        let wolf = units.spawned_at(-18, 12);
        let home = ground(-18, 12).get();
        match tick {
            2460 => self.wolf_answers = target(wolf),
            2500..2620 => {
                let away = units.position(wolf).get().distance(home);
                self.wolf_farthest = self.wolf_farthest.max(away);
            }
            2640 => self.wolf_home = Some((target(wolf), units.position(wolf))),
            2993 | 2994 => self.xp_at_wolf_fall.push(xp()),
            _ => {}
        }
        if [1200, 1201, 1900].contains(&tick) {
            let [cinder, .., rime, veil] = heroes;
            let learning = [cinder, veil, rime].map(|hero| Learning::of(world, hero));
            self.learning.push(learning);
        }
        if tick == SKIRMISH - 1 {
            self.gold = (0..Reference3v3::PLAYERS)
                .map(|slot| gold(world, reference, slot))
                .collect();
        }
    }
}

fn ground(x: i64, z: i64) -> Position {
    let meters = |value| Num::from_int(value).unwrap();
    Position::new(Vec3::new(meters(x), Num::ZERO, meters(z))).unwrap()
}

/// The unit that stood at `x`, `z` at the pick's end: a structure or a camp.
fn placed(run: &Run, x: i64, z: i64) -> StableId {
    let at = ground(x, z);
    let unit = run.at_pick_end.iter().find(|unit| unit.pos == at);
    unit.expect("a placed unit").id
}

#[test]
fn a_3v3_match_replays_to_the_same_hashes() {
    let reference = Reference3v3::load();
    let run = run(&reference);
    run.golden.check("3v3");
    let melee = assert_start(&reference, &run);
    assert_skirmish(&run);
    assert_camp(&run);
    assert_gold(&run, melee);
    assert_learning(&run);
    showcase::assert_farm(&run.showcase);
    showcase::assert_shop(&reference, &run.showcase);
    let world = run.fixed.runner().world();
    showcase::assert_casts(&reference, &run.showcase, world, TICKS);
    assert_replays(&reference, run.fixed.runner(), &run.trail);
}

/// The match's state at its end, its scripts, and its units at the pick's end and as the first
/// wave spawns; the unit type of the first wave's melee creeps.
fn assert_start(reference: &Reference3v3, run: &Run) -> UnitType {
    let world = run.fixed.runner().world();
    // State in the order of its fields' names.
    let packages = reference.packages();
    let at = packages
        .data()
        .state
        .keys()
        .position(|name| name.as_str() == "phase");
    let phase = &world.resource::<ModeState>().get()[at.unwrap()];
    assert_eq!(phase, &StateValue::Text("play".to_owned()));
    // Each script file compiles once, however many abilities or unit types run it.
    let scripts = packages.packages().map(|view| view.package.scripts.len());
    assert_eq!(
        world.non_send::<ScriptHost>().compiled(),
        scripts.sum::<usize>()
    );

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
    assert_eq!(
        heroes,
        (0..Reference3v3::PLAYERS).map(hero).collect::<Vec<_>>()
    );
    // Each hero's slots, kind after kind: its three basic abilities and its ultimate, unlearned,
    // then the two spells its player chose, haste and mend, learned from the spawn: the same
    // two abilities for every hero; its weapon, learned from the spawn; and last the six slots of
    // its inventory, empty, of one rank.
    let [basic, ultimate, spell, weapon, item] = [0, 1, 2, 3, 4].map(SlotKind::new);
    let spells = &run.at_pick_end[14].slots[4..6];
    for unit in &run.at_pick_end[14..20] {
        let slots: Vec<_> = unit
            .slots
            .iter()
            .map(|slot| (slot.kind, slot.rank))
            .collect();
        let kinds = [
            (basic, 0),
            (basic, 0),
            (basic, 0),
            (ultimate, 0),
            (spell, 1),
            (spell, 1),
            (weapon, 1),
            (item, 1),
            (item, 1),
            (item, 1),
            (item, 1),
            (item, 1),
            (item, 1),
        ];
        assert_eq!(slots, kinds);
        assert!(unit.slots[7..].iter().all(|slot| slot.action.is_none()));
        assert_eq!(&unit.slots[4..6], spells);
    }
    assert_ne!(spells[0].action, spells[1].action);
    // Each camp on its marker, in the map's order.
    let camps: Vec<_> = run.at_pick_end[20..]
        .iter()
        .map(|unit| (unit.team, unit.pos))
        .collect();
    let markers = packages.map().markers.iter();
    let camp_markers =
        markers.filter(|marker| marker.tags.iter().any(|tag| tag.as_str() == "camp"));
    let neutral = Team::new(2);
    let expected: Vec<_> = camp_markers
        .map(|marker| (neutral, marker.pos.unwrap().position().unwrap()))
        .collect();
    assert_eq!(expected.len(), 5);
    assert_eq!(camps, expected);
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
    wave[0].kind
}

/// The skirmish: first blood and its assist, a tower's kill, the experience a hero's fall
/// shares, the respawns, mend, haste, and the tower that turns on the diver.
fn assert_skirmish(run: &Run) {
    let heroes = [0, 1, 2, 3, 4, 5].map(|slot| hero_of(run, slot));
    let [cinder, gale, husk, _, rime, _] = heroes;
    let tower = placed(run, -42, 16);
    // Cinder fells Rime in tick 1840, Gale assisting, and the tower fells Husk in tick 2311,
    // with no hero assisting. A hero of level 1 comes back 5000 + 2500 × 1 ms, 150 ticks, after
    // the end of the tick it fell in.
    let fallen: Vec<&Death> = run
        .deaths
        .iter()
        .filter(|death| death.tick < SKIRMISH && heroes.contains(&death.unit))
        .collect();
    let rime_fell = Death {
        tick: 1840,
        unit: rime,
        killer: Some(cinder),
        assisters: vec![gale],
        respawn: Some(Tick::new(1840 + 1 + 150)),
    };
    let husk_fell = Death {
        tick: 2311,
        unit: husk,
        killer: Some(tower),
        assisters: Vec::new(),
        respawn: Some(Tick::new(2311 + 1 + 150)),
    };
    assert_eq!(fallen, [&rime_fell, &husk_fell]);

    // A fallen hero of level 1 shares 150 + 25 × 1 experience among the enemy heroes within
    // 16 m: Rime's between Cinder and Gale, 87.5 each; Husk's to Kensho, beside the tower.
    let half = Num::int(175) / 2;
    let xp = [half, half, Num::ZERO, Num::int(175), Num::ZERO, Num::ZERO];
    assert_eq!(run.seen.xp_after_skirmish, xp);

    // Mend heals Gale by 75 + 15 × (1 − 1) in tick 1880, beside the regen of every tick; haste,
    // cast in tick 1200, lasts 10 s, 200 ticks, and its end comes as tick 1401 starts.
    let gale = &run.seen.gale;
    let regen = gale[1] - gale[0];
    assert_eq!(gale[2] - gale[1] - regen, Num::int(75));
    assert_eq!(run.seen.hasted, [true, false]);

    // The tower holds Cinder, the nearest hero in its range, until Husk strikes Kensho, which
    // turns it on Husk within one think of 250 ms, 5 ticks.
    let tower = &run.seen.tower;
    let struck = tower
        .iter()
        .position(|&(_, kensho)| kensho < Num::int(444))
        .unwrap();
    assert_eq!(tower[struck - 1].0, Some(cinder));
    assert_eq!(tower[struck + 5].0, Some(husk));
}

/// The south camp: its wolf answers Kensho, who struck it, chases him south past its 8 m leash
/// and goes home; then he fells it, Rime assisting.
fn assert_camp(run: &Run) {
    let [kensho, rime] = [3, 4].map(|slot| hero_of(run, slot));
    let wolf = placed(run, -18, 12);
    assert_eq!(run.seen.wolf_answers, Some(kensho));
    assert!(
        run.seen.wolf_farthest > Num::int(8),
        "{}",
        run.seen.wolf_farthest
    );
    assert_eq!(run.seen.wolf_home, Some((None, ground(-18, 12))));
    // It falls in tick 2994 and comes back 60 s, 1200 ticks, after the end of that tick; its 90
    // experience is split between the two heroes within 16 m, 45 each, in that tick alone.
    let fell = run.deaths.iter().find(|death| death.unit == wolf);
    let expected = Death {
        tick: 2994,
        unit: wolf,
        killer: Some(kensho),
        assisters: vec![rime],
        respawn: Some(Tick::new(2994 + 1 + 1200)),
    };
    assert_eq!(fell, Some(&expected));
    let [before, after] = [0, 1].map(|at| &run.seen.xp_at_wolf_fall[at]);
    let gained: Vec<Num> = after.iter().zip(before).map(|(a, b)| *a - *b).collect();
    let share = Num::int(45);
    let zero = Num::ZERO;
    assert_eq!(gained, [zero, zero, zero, share, share, zero]);
}

/// The players' gold at the skirmish's end: the income, first blood and its assist, the wolf's bounty, and
/// the bounty of each creep of the first wave a hero felled, by its kind, `melee` or a caster's.
fn assert_gold(run: &Run, melee: UnitType) {
    // A creep a hero fells pays its player 20 for a melee creep, the first three of a group, and
    // 15 for a caster: Cinder fells a caster and a melee creep of the south's west group, 35, and
    // Veil a caster of the north's east group, 15.
    let [cinder, veil] = [0, 5].map(|slot| hero_of(run, slot));
    let bounty = |killer| -> i64 {
        let felled = run
            .deaths
            .iter()
            .filter(|death| death.tick < SKIRMISH && death.killer == Some(killer));
        felled
            .filter_map(|death| {
                let creep = run
                    .at_first_wave
                    .iter()
                    .find(|unit| unit.id == death.unit && unit.walks)?;
                Some(if creep.kind == melee { 20 } else { 15 })
            })
            .sum()
    };
    assert_eq!([bounty(cinder), bounty(veil)], [35, 15]);

    // Income: 8 gold every 5 s from the pick's end, 100 ticks, in ticks 1299 to 3199: 20 times,
    // 160. Cinder's player takes first blood, 300 + 100, and the creeps' 35; Gale's the assist's
    // 150, split among one assister; Kensho's the wolf's 30; Veil's the creep's 15.
    assert_eq!(
        run.seen.gold,
        [160 + 435, 160 + 150, 160, 160 + 30, 160, 160 + 15]
    );
}

/// Learning: each hero spawns with a point, which a learn spends on a rank its level allows.
fn assert_learning(run: &Run) {
    let learning = |ranks, points, dead| Learning {
        ranks,
        points,
        dead,
    };
    // Tick 1200: Cinder learns Fire Lance with her point, and her second learn finds none; Veil's
    // ultimate needs level 6, so her point stays. Tick 1201: Veil learns Dusk Mark. Tick 1900,
    // between Rime's fall in tick 1840 and her return in tick 1991: she learns Fan of Frost.
    let [cinder_learned, veil_unlearned, rime_unlearned] = [
        learning([1, 0, 0, 0], 0, false),
        learning([0, 0, 0, 0], 1, false),
        learning([0, 0, 0, 0], 1, false),
    ];
    let veil_learned = learning([1, 0, 0, 0], 0, false);
    let expected = [
        [cinder_learned, veil_unlearned, rime_unlearned],
        [cinder_learned, veil_learned, rime_unlearned],
        [
            cinder_learned,
            veil_learned,
            learning([0, 1, 0, 0], 0, true),
        ],
    ];
    assert_eq!(run.seen.learning, expected);
    // At the end, a hero's points are its levels less the ranks it learned.
    let world = run.fixed.runner().world();
    for slot in 0..Reference3v3::PLAYERS {
        let hero = hero_of(run, slot);
        let learned = Learning::of(world, hero);
        let entity = world.resource::<EntityIndex>().get(hero).unwrap();
        let level = world.get::<Level>(entity).unwrap().get();
        let ranks: u32 = learned.ranks.iter().map(|&rank| u32::from(rank)).sum();
        assert_eq!(learned.points, level - ranks, "slot {slot}");
    }
}

/// The match of `runner`, which recorded `trail`, its log replayed from its file, gives its hash
/// after every tick and ends where it ended.
pub(crate) fn assert_replays(reference: &Reference3v3, runner: &Runner, trail: &HashTrail) {
    let mut file = Vec::new();
    runner.log().encode(&mut file);
    let decoded = SessionLog::decode(&file).unwrap();
    let mut replay = Runner::new(
        decoded.rewound(),
        Reference3v3::seed(),
        reference.packages(),
    )
    .unwrap();
    let mut replayed = HashTrail::default();
    for _ in trail.totals() {
        replay.run_tick();
        replayed.record(replay.world());
    }
    trail.assert_same(&replayed);
    assert_eq!(replay.log().next_tick(), runner.log().next_tick());
}
