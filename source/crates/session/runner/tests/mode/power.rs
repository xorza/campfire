use bevy_ecs::entity_disabling::Disabled;
use campfire_actions::ProgressDues;
use campfire_production::{PlayerPower, ProductionRefusal, Site, SiteProgress, SiteStart, Sold};
use campfire_script::CapabilityRefusal;
use campfire_sim::internals::SimWorldInternals;
use campfire_stats::StatsWorld;
use campfire_values::Block;

use super::*;

/// A script that writes its player's power given and used to `kind` on the input `read`.
const READ: &str = r#"
fn on_input(ctx, player, name, value) {
    if value == "read" {
        ctx.state.kind = `${player.power_given} ${player.power_used}`;
    }
}
"#;

/// The test mode of `script` that counts power: its stat `power`; the tag `powered`, and
/// `disabled`, which blocks moving and attacks; `underpowered`, the modifier a short player holds
/// on its powered units; `rods`, a player modifier that adds 5 to its grunts' power; the mode's
/// `[power]` of `speed`, a line of its own or none; a plant of the grunt's tag that gives 5, and
/// a cannon, powered, that takes 3 (power.md, Power is production's).
fn powered(script: &str, speed: &str) -> PackageTree {
    let mut set = mode_packages(script);
    set.edit(DATA, Edit::Set("stats.power", "{}"));
    let rules = format!(
        "\n[tags.powered]\n\n[tags.disabled]\nblocks = [\"move\", \"attack\"]\n\n\
         [modifiers.underpowered]\nreapply = \"refresh\"\naffects = \"allies:powered\"\n\
         tags = [\"disabled\"]\n\n[modifiers.rods]\nreapply = \"refresh\"\n\
         affects = \"allies:grunt\"\nstats = {{ power = 5 }}\n\n\
         [power]\nstat = \"power\"\nshort = \"underpowered\"\n{speed}"
    );
    set.edit(DATA, Edit::Append(&rules));
    set.edit(
        UNITS,
        Edit::Append(
            "\n[units.plant]\ntags = [\"grunt\"]\npools = [\"health\"]\n\
             stats = { health = { base = 10 }, power = { base = 5 } }\n\
             combat = { on_death = \"stay\" }\n\n[units.cannon]\ntags = [\"powered\"]\n\
             pools = [\"health\"]\n\
             stats = { health = { base = 10 }, move_speed = { base = 10 }, power = { base = -3 } }\n\
             combat = { on_death = \"stay\" }\n",
        ),
    );
    set
}

/// A unit of the type `name` that player `owner` owns, at the origin.
fn owned(game: &mut Match, name: &str, owner: u32) -> StableId {
    let owner = Owner::new(PlayerSlot::new(owner));
    Spawn::named(game, name, Vec3::ZERO)
        .parts(owner)
        .place(game)
}

/// Player `slot`'s power as its entity shows it, given and used, in whole units.
fn power(game: &Match, slot: u32) -> (i64, i64) {
    let entity = game.player_entity(PlayerSlot::new(slot)).unwrap();
    let power = game.get::<PlayerPower>(entity).copied().unwrap();
    (power.given().floor(), power.used().floor())
}

#[test]
fn a_players_power_counts_its_living_complete_units_and_a_shortage_holds_the_modes_modifier() {
    let mut game = started(&powered(READ, ""), ScriptLimits::ROOMY).unwrap();
    let underpowered = Stats::modifier(&game, PackageIndex::MODE, "underpowered").unwrap();
    let rods = Stats::modifier(&game, PackageIndex::MODE, "rods").unwrap();
    let holds = |game: &mut Match, slot: u32| {
        let held = game.player_modifiers_mut(PlayerSlot::new(slot)).unwrap();
        held.holds(underpowered)
    };
    let blocked = |game: &Match, id: StableId| {
        let tags = game.try_part::<UnitTags>(id);
        tags.is_some_and(|tags| tags.properties.blocks(Block::Move))
    };
    Mode::tick_inputs(&mut game, &[]);
    // A plant gives 5, a cannon takes 3: not short, as each tick's end counts them.
    let plant = owned(&mut game, "plant", 0);
    let first = owned(&mut game, "cannon", 0);
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(power(&game, 0), (5, 3));
    assert!(!holds(&mut game, 0));
    // A second cannon: 6 of 5, short from this tick's end; its cannons hold the modifier's tag,
    // which blocks moving, from the next tick's grants, and the plant, not powered, never does.
    let second = owned(&mut game, "cannon", 0);
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(power(&game, 0), (5, 6));
    assert!(holds(&mut game, 0));
    assert!(!blocked(&game, first));
    Mode::tick_inputs(&mut game, &[(0, input("phase", "read"))]);
    assert_eq!(Mode::field(&game, "kind"), StateValue::Text("5 6".into()));
    assert_eq!(
        [first, second, plant].map(|id| blocked(&game, id)),
        [true, true, false]
    );
    // The rods add 5 to the plant's stat from the next grants: 10 of 6, so the modifier goes as
    // that tick ends, and the cannons move again from the tick after.
    game.player_modifiers_mut(PlayerSlot::new(0))
        .unwrap()
        .add(rods);
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(power(&game, 0), (10, 6));
    assert!(!holds(&mut game, 0));
    Mode::tick_inputs(&mut game, &[]);
    assert!(!blocked(&game, first));
    // A dead cannon takes nothing; one that rides, out of the world, still takes its 3, and so
    // does one under sale until it goes; a site takes nothing until it completes.
    game.insert_parts(first, Dead);
    game.insert_parts(second, Disabled);
    let sold = owned(&mut game, "cannon", 0);
    let now = game.now();
    game.insert_parts(sold, Sold::new(now, Ticks::new(100)));
    let site = owned(&mut game, "cannon", 0);
    let started = Site::new(SiteStart {
        action: ActionId::new(0),
        rank: Rank::FIRST,
        time: Num::int(4),
        gain: Num::ZERO,
        paid: &[],
        holder: None,
    });
    game.insert_parts(site, (started, SiteProgress::default()));
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(power(&game, 0), (10, 6));
    // Player 1 captures the plant, which its rods no longer reach: player 0 gives none of the 6
    // it takes and is short; player 1 gives the plant's own 5. Player 2, of no power and no use,
    // is not short.
    game.insert_parts(plant, Owner::new(PlayerSlot::new(1)));
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!([power(&game, 0), power(&game, 1)], [(0, 6), (5, 0)]);
    assert_eq!(
        [0, 1, 2].map(|slot| holds(&mut game, slot)),
        [true, false, false]
    );
    assert_eq!(power(&game, 2), (0, 0));
    // A short player that lost the modifier, as a script or a restore may leave it, holds it
    // again as the next tick ends.
    game.player_modifiers_mut(PlayerSlot::new(0))
        .unwrap()
        .remove(underpowered);
    Mode::tick_inputs(&mut game, &[]);
    assert!(holds(&mut game, 0));
    assert_eq!(Mode::failures(&mut game), []);
    // A mode that counts no power fails the call.
    let mut uncounted = mode_match(READ, ScriptLimits::ROOMY);
    Mode::tick_inputs(&mut uncounted, &[(0, input("phase", "read"))]);
    assert_eq!(
        Mode::failures(&mut uncounted),
        [FailureKind::Api(ProductionRefusal::NoPower.refused())]
    );
}

#[test]
fn a_trains_end_follows_its_players_power_from_the_tick_it_reached_the_head() {
    // A barracks of player 0 trains a grunt in 1500 ms, 15 ticks at 10 a second, and researches
    // the drill in as long; Zero Hour's speed slows trains and builds, and a player of no power
    // and no use builds at 0.5: 30 ticks. An order in tick o starts the head at time o, so a head
    // of t ticks makes its unit in tick o + t − 1.
    let speed = "speed = { penalty = 1, min = \"0.5\", max = \"0.8\", empty = \"none\", slows = \
                 [\"train\", \"build\"] }\n";
    let mut set = powered(READ, speed);
    set.edit(
        DATA,
        Edit::Append(
            "\n[actions.train_grunt]\nname = \"action-train-grunt\"\nkind = \"train\"\n\
             targeting = \"none\"\nwindup_ms = 1500\nunit_type = \"grunt\"\n\n\
             [actions.research_drill]\nname = \"action-research-drill\"\nkind = \"research\"\n\
             targeting = \"none\"\nwindup_ms = 1500\nmodifier = \"drill\"\n",
        ),
    );
    set.edit(
        LOCALE,
        Edit::Append("action-train-grunt = Grunt\naction-research-drill = Drill\n"),
    );
    set.edit(UNITS, Edit::Append(&stat_type("barracks", &[])));
    set.edit(
        UNITS,
        Edit::Append(
            "production = { queue = 2 }\nslots = { spell = [\"train_grunt\", \"research_drill\"] }\n",
        ),
    );
    // The tick of the order, and the tick its unit or modifier came, of a fresh match in which
    // `plant_at`, ticks after the order, a plant joins player 0, and slot `slot` was ordered.
    let made = |slot: u8, plant_at: Option<u64>| {
        let mut game = started(&set, ScriptLimits::ROOMY).unwrap();
        let barracks = owned(&mut game, "barracks", 0);
        let producer = game.unit_entity(barracks);
        Mode::tick_inputs(&mut game, &[]);
        let ordered = game.now().get();
        ProgressDues::change(&mut game, barracks, producer, |progress, dues| {
            progress.order(slot, ActionTarget::None, dues);
        })
        .unwrap();
        let drill = Stats::modifier(&game, PackageIndex::MODE, "drill").unwrap();
        let grunt = Units::type_named(&game, "grunt").unwrap();
        for ran in 0..40 {
            if plant_at == Some(ran) {
                owned(&mut game, "plant", 0);
            }
            let tick = game.now().get();
            Mode::tick_inputs(&mut game, &[]);
            let mut grunts = game.query::<&UnitType>();
            let trained = grunts.iter(&game).any(|&unit_type| unit_type == grunt);
            let researched = game
                .player_modifiers_mut(PlayerSlot::new(0))
                .unwrap()
                .holds(drill);
            if trained || researched {
                return tick - ordered;
            }
        }
        panic!("nothing came in 40 ticks");
    };
    // Of no power, the train takes 30 ticks: made 29 ticks after its order's tick.
    assert_eq!(made(0, None), 29);
    // A plant from the start gives 5 of none used: full speed, 15 ticks.
    assert_eq!(made(0, Some(0)), 14);
    // A plant counted as tick o + 10 ends: the head of 15 ticks from time o ends at time o + 15,
    // as at full speed. Counted as tick o + 16 ends, past that end: made in the next tick, at
    // once, as the original's share of the frames spent jumps.
    assert_eq!(made(0, Some(10)), 14);
    assert_eq!(made(0, Some(16)), 17);
    // A research, which the speed does not slow, takes its 15 ticks of no power.
    assert_eq!(made(1, None), 14);
}
