//! A weapon's rate bonus and its shot streak (04-capabilities/combat.md, A weapon's own
//! period may speed up by a stat; 04-capabilities/combat.md, A shot streak gives a stage's
//! modifier).

use campfire_stats::{StatChange, StatOp};
use campfire_values::{Number, Scalar};

use super::*;

/// The ticks a stage's coast lasts past its weapon's ready tick.
const COAST: u64 = 60;

/// The stat the gun's period divides by, the third of the match's stats by name.
const RATE_OF_FIRE: StatId = StatId::new(2);

/// A gun's match, a server's or of `role`: the weapon stats and `rate_of_fire`, and the modifiers
/// `mean` and `fast`, which add 1 and 2 to it.
fn streak_match(role: Option<Role>) -> Match {
    let builder = Match::builder(&[Capability::Stats, Capability::Combat]);
    let mut game = match role {
        Some(role) => builder.role(role).build(),
        None => builder.build(),
    };
    let rules = ["attack_rate", "damage", "rate_of_fire"]
        .map(|name| (Stat::named(name).unwrap(), StatRule::default()));
    Stats::load_rules(&mut game, &BTreeMap::from(rules));
    for (name, add) in [("fast", 2), ("mean", 1)] {
        let change = StatChange {
            op: StatOp::Add,
            value: Number::Value(Scalar::Int(add)),
        };
        let data = ModifierData {
            stats: BTreeMap::from([(Stat::named("rate_of_fire").unwrap(), change)]),
            ..ModifierData::default()
        };
        Stats::load_modifier(&mut game, PackageIndex::MODE, name, &data, None);
    }
    game
}

/// The modifier `name` of the match.
fn modifier(game: &Match, name: &str) -> ModifierId {
    Stats::modifier(game, PackageIndex::MODE, name).unwrap()
}

/// A gun of 1 damage within 2 m, no windup and its own period of 8 ticks, sped by
/// `rate_of_fire`, with a streak of stages above 1 and 5 shots, `mean` and `fast`, as Zero Hour's
/// Gattling Cannon's; it fires from x = 4 at a dummy of 100 life 2 m off, which holds `held`. The
/// gunner holds the three stats at 0.
fn gun_fight(game: &mut Match, held: impl Bundle<Effect: NoBundleEffect>) -> [StableId; 2] {
    let [dummy] = load_types(game, [dummy()]);
    let streak = Streak::new(
        Ticks::new(COAST),
        &[(1, modifier(game, "mean")), (5, modifier(game, "fast"))],
    );
    let weapon = TestWeapon {
        rate: Rate::Period(Box::new([Ticks::new(8)])),
        rate_bonus: Some(BonusStats::from([RATE_OF_FIRE])),
        damage: WeaponDamage::Amount(Box::new([Num::ONE])),
        streak: Some(streak),
        ..TestWeapon::new(
            Filter::of_match_relations(game, RelationSet::Enemies),
            ActionRange::meters(Num::int(2)),
            Ticks::ZERO,
        )
    };
    let gun = Combat::add_weapon(game, weapon);
    let slots = ActionSlots::new([(gun, SlotKind::new(0), Rank::new(1))]);
    let gunner = wielder(game, slots, &[Num::ZERO; 3]);
    let gunner = Spawn::of(gunner, (4, 0, 0)).place(game);
    let dummy = Spawn::of(dummy, (6, 0, 0))
        .team(Team::new(1))
        .parts(held)
        .place(game);
    Combat::attack(game, gunner, dummy);
    [gunner, dummy]
}

/// The modifiers `id` holds from itself.
fn held(game: &Match, id: StableId) -> Vec<ModifierId> {
    let held = Stats::carried_modifiers(game, id).into_iter();
    held.filter(|&(_, source)| source == Some(id))
        .map(|(modifier, _)| modifier)
        .collect()
}

#[test]
fn a_streak_speeds_its_weapon_a_stage_at_a_time_and_its_coast_ends_it() {
    // The second shot passes 1, so the gun holds `mean` from tick 8: 8 ÷ (1 + 1) = 4 ticks. The
    // sixth, in tick 28, passes 5, so it holds `fast` in its place: ⌊8 ÷ (1 + 2)⌋ = 2. Each
    // period is read as the shot goes off, before it counts, so the hits come in ticks 0, 8, 16,
    // 20, 24, 28, 32 and 34.
    let mut game = streak_match(None);
    let [gunner, dummy] = gun_fight(&mut game, ());
    let [mean, fast] = [modifier(&game, "mean"), modifier(&game, "fast")];
    let mut health = Stats::health(&game, dummy);
    let (mut hits, mut stages) = (Vec::new(), Vec::new());
    for tick in 0..36 {
        game.run_until(tick + 1);
        let now = Stats::health(&game, dummy);
        if now != health {
            hits.push(tick);
            health = now;
        }
        let holds = held(&game, gunner);
        if stages.last().map(|(_, last)| last) != Some(&holds) {
            stages.push((tick, holds));
        }
    }
    assert_eq!(hits, [0, 8, 16, 20, 24, 28, 32, 34]);
    assert_eq!(stages, [(0, vec![]), (8, vec![mean]), (28, vec![fast])]);
    // The last shot, in tick 34, is ready again in 36, so the coast ends in 36 + 60 = 96: with the
    // dummy gone, `fast` holds through tick 96 and goes in tick 97.
    let entity = game.unit_entity(dummy);
    game.despawn(entity);
    game.run_until(97);
    assert_eq!(held(&game, gunner), [fast]);
    game.run_until(98);
    assert_eq!(held(&game, gunner), []);
}

#[test]
fn a_server_counts_each_units_streak_and_a_client_none() {
    // On a server, every unit with slots holds its streak from its spawn, the dummy's empty; a
    // predicting client holds none, and its gun's period reads only the modifiers the server
    // sends, so with none it fires each 8 ticks.
    let mut game = streak_match(None);
    let [gunner, dummy] = gun_fight(&mut game, ());
    assert_eq!(
        game.try_part::<ShotStreak>(dummy),
        Some(&ShotStreak::default())
    );
    game.run_until(1);
    assert_ne!(
        game.try_part::<ShotStreak>(gunner),
        Some(&ShotStreak::default())
    );

    let mut client = streak_match(Some(Role::Client));
    let [gunner, _] = gun_fight(&mut client, Unpredicted);
    assert_eq!(client.try_part::<ShotStreak>(gunner), None);
    let mut readies = Vec::new();
    for tick in 0..20 {
        client.run_until(tick + 1);
        let ready = Attack::of(&client, gunner).ready_at().get();
        if readies.last() != Some(&ready) {
            readies.push(ready);
        }
    }
    assert_eq!(readies, [8, 16, 24]);
}
