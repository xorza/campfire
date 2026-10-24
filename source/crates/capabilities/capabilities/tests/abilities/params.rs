use std::collections::BTreeMap;

use campfire_abilities::internals::scaling_stats;
use campfire_abilities::*;
use campfire_actions::{ActionData, ActionKeys, ActionSlots, ActionTarget, SlotKind, TimedSlots};
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Release};
use campfire_common::{DeclaredName, PlayerSlot, Tick};
use campfire_math::{Num, Vec3};
use campfire_script::ScriptLimits;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, SimWorld, StableId, StateCopy};
use campfire_stats::{
    CarriedMut, Level, LiveCarriers, ModifierClocks, ModifierData, Modifiers, StatBook, StatGraph,
    StatRule, StatValue, Stats, StatsData, UnitStats,
};
use campfire_units::internals::Spawn;
use campfire_units::{Owner, UnitTypeCore, Units};
use campfire_values::{NameId, PackageIndex, Rank, Ranked, Stat, Team, TypeScope, UnitType};

use crate::abilities::{abilities_match_with, changing, int, param, scaling, strike};

fn decimal(text: &str) -> Num {
    text.parse().unwrap()
}

#[test]
fn a_scaling_param_reads_its_sources_level_stats_and_bonus() {
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &declared);
    // The caster's type gives attack damage 50 + 5 a level, so 60 at level 3; a modifier adds 20
    // more and 40 ability power.
    let caster_type = Units::load_type(
        &mut game,
        TypeScope::Mode,
        "scaler",
        &UnitTypeCore::default(),
    );
    Abilities::give_caster(&mut game, caster_type, 100, 20);
    let attack_damage = Stat::named("attack_damage").unwrap();
    let growth = StatValue {
        base: Num::from_int(50).unwrap(),
        per_level: Num::from_int(5).unwrap(),
    };
    let growth = StatsData::from([(attack_damage, growth)]);
    let rules: BTreeMap<_, _> = scaling_stats()
        .map(|stat| (stat, StatRule::default()))
        .into();
    let mut book = StatBook::new(&rules, Num::int(6));
    book.add_type(caster_type, &growth).unwrap();
    Stats::load_book(&mut game, book);
    let boost = changing(
        &[("attack_damage", int(20)), ("ability_power", int(40))],
        &[],
        None,
    );
    Stats::load_modifier(&mut game, PackageIndex::MODE, "boost", &boost, None);
    let mark = ModifierData {
        shield: Some(param("power")),
        ..ModifierData::default()
    };
    Stats::load_modifier(&mut game, PackageIndex::MODE, "mark", &mark, None);
    // Power: 100 a rank, 10 a level, half the ability power and 1.5 times the bonus attack
    // damage.
    let half = Num::HALF;
    let power = scaling(
        Ranked::PerRank([100, 200, 300, 400, 500].map(Num::int).to_vec()),
        10,
        &[("ability_power", half)],
        &[("attack_damage", half * 3)],
    );
    let data = ActionData {
        keys: ActionKeys {
            params: [(DeclaredName::new("power").unwrap(), power)].into(),
            ..strike().keys
        },
        ..strike()
    };
    let script = r#"
fn on_resolve(ctx, caster, target) {
    ctx.damage(target, "true", ctx.params.power);
    ctx.add_modifier(caster, "mark");
}
"#;
    let action = Abilities::load(&mut game, "ability", &data, script);
    let parts = (
        Owner::new(PlayerSlot::new(0)),
        TimedSlots::new(ActionSlots::new([(action, SlotKind::new(0), Rank::new(2))])),
        Level::new(3).unwrap(),
    );
    let caster = Spawn::of(caster_type, Vec3::ZERO)
        .parts(parts)
        .place(&mut game);
    Abilities::give(&mut game, caster, "boost");
    let target = Abilities::spawn_combatant(&mut game, (5, 0, 0), Team::new(1), ());
    Abilities::cast(&mut game, caster, ActionTarget::Unit(target));

    // At rank 2 and level 3: 200 + 10 × 2 + 0.5 × 40 + 1.5 × (80 − 60) = 270, which the script
    // deals, 500 → 230, and the shield of the caster's mark holds.
    assert_eq!(Abilities::failed_calls(&mut game), []);
    assert_eq!(Stats::health(&game, target), 230);
    let mark = Stats::modifier(&game, PackageIndex::MODE, "mark").unwrap();
    let entity = game.unit_entity(caster);
    let modifiers = game.get::<Modifiers>(entity).unwrap();
    let clocks = game.get::<ModifierClocks>(entity).unwrap();
    let shield = clocks.shield_of(modifiers, mark, Some(caster));
    assert_eq!(shield, Some(Num::int(270)));
}

/// Veil, her type, and the places of attack damage, spell vamp and armor: Veil has attack damage 53
/// at level 1. Dual Path gives her spell vamp of 0.06 and 0.00167 a point of bonus attack damage;
/// Fortify gives armor of 10 times her spell vamp, so armor reads spell vamp, which reads attack
/// damage. Armor's place, before spell vamp's, makes the graph's order differ from the places'.
/// Veil holds both, and Boost of 30 attack damage is loaded.
#[derive(Debug, Clone, Copy)]
struct Veil {
    id: StableId,
    unit_type: UnitType,
    places: [usize; 3],
}

impl Veil {
    /// The capabilities Veil's match declares.
    const CAPABILITIES: [Capability; 3] =
        [Capability::Stats, Capability::Combat, Capability::Abilities];

    /// Loads Veil's type, stats and modifiers into `game`, a match of `CAPABILITIES`, and spawns
    /// her.
    fn place(game: &mut Match) -> Self {
        let veil_type = Units::load_type(game, TypeScope::Mode, "game", &UnitTypeCore::default());
        Abilities::give_combatant(game, veil_type);
        let attack_damage = Stat::named("attack_damage").unwrap();
        let growth = StatValue {
            base: Num::from_int(53).unwrap(),
            per_level: Num::ZERO,
        };
        let growth = StatsData::from([(attack_damage.clone(), growth)]);
        let [spell_vamp, armor] = ["spell_vamp", "armor"].map(|name| Stat::named(name).unwrap());
        let mut graph = StatGraph::new(scaling_stats());
        graph.add(&attack_damage, &spell_vamp).unwrap();
        graph.add(&spell_vamp, &armor).unwrap();
        let rules: BTreeMap<_, _> = scaling_stats()
            .map(|stat| (stat, StatRule::default()))
            .into();
        let mut book = StatBook::new(&rules, Num::int(6));
        book.add_type(veil_type, &growth).unwrap();
        let book = book.with_order(graph.order().unwrap());
        let place = |stat: &Stat| book.named(stat).unwrap().index();
        let places = [&attack_damage, &spell_vamp, &armor].map(place);
        Stats::load_book(game, book);
        let vamp = scaling(
            Ranked::One(decimal("0.06")),
            0,
            &[],
            &[("attack_damage", decimal("0.00167"))],
        );
        let dual_path = changing(&[("spell_vamp", param("vamp"))], &[("vamp", vamp)], None);
        let guard = scaling(
            Ranked::One(Num::ZERO),
            0,
            &[("spell_vamp", Num::int(10))],
            &[],
        );
        let fortify = changing(&[("armor", param("guard"))], &[("guard", guard)], None);
        let boost = changing(&[("attack_damage", int(30))], &[], None);
        for (name, data) in [
            ("boost", boost),
            ("dual_path", dual_path),
            ("fortify", fortify),
        ] {
            Stats::load_modifier(game, PackageIndex::MODE, name, &data, None);
        }
        let veil = Spawn::of(veil_type, Vec3::ZERO).place(game);
        Abilities::give(game, veil, "dual_path");
        Abilities::give(game, veil, "fortify");
        Self {
            id: veil,
            unit_type: veil_type,
            places,
        }
    }

    /// The attack damage, spell vamp and armor of `id` in `game`.
    fn values(self, game: &Match, id: StableId) -> [Num; 3] {
        let stats = game.part::<UnitStats>(id).values();
        self.places.map(|at| stats[at])
    }
}

#[test]
fn a_live_change_follows_its_source_in_the_order_of_the_stats_it_reads() {
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &Veil::CAPABILITIES);
    let veil = Veil::place(&mut game);
    let unit = veil.id;
    game.step();
    // 0.06 is 1 006 632.96 bits, to 1 006 633; no bonus; armor 10 times that.
    assert_eq!(
        veil.values(&game, unit),
        [
            Num::int(53),
            Num::from_bits(1_006_633),
            Num::from_bits(10_066_330)
        ]
    );

    // 30 more attack damage: in the same refresh, bonus 30, and 0.00167, 28 017.95 bits to
    // 28 018, times 30 is 840 540: spell vamp 1 847 173 bits, armor ten times it.
    Abilities::give(&mut game, unit, "boost");
    game.step();
    assert_eq!(
        veil.values(&game, unit),
        [
            Num::int(83),
            Num::from_bits(1_847_173),
            Num::from_bits(18_471_730)
        ]
    );
}

#[test]
fn a_live_change_keeps_its_last_value_when_its_source_is_gone_and_none_at_no_stack() {
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &Veil::CAPABILITIES);
    let veil = Veil::place(&mut game);
    let source = veil.id;
    // Ward, of Veil's type, holds Dual Path from Veil, with Boost: 0.06 and 0.00167 of Veil's
    // bonus of 30, as above, spell vamp 1 847 173 bits.
    let ward = Spawn::of(veil.unit_type, (1, 0, 0)).place(&mut game);
    Abilities::give(&mut game, source, "boost");
    Abilities::give_from(&mut game, ward, source, "dual_path");
    // A copy of the state follows the match: the value a live change last had is state, and
    // each write of it marks its change.
    let mut copy = StateCopy::of(&mut game);
    game.step();
    Release::check_copy(&mut game, &mut copy);
    let vamp = Num::from_bits(1_847_173);
    assert_eq!(veil.values(&game, ward)[1], vamp);
    // Veil's own spell vamp, and Ward's, are live changes of Veil, so both refresh as Veil does.
    let ward_entity = game.unit_entity(ward);
    let mut carriers = [(source, game.unit_entity(source)), (source, ward_entity)];
    carriers.sort_unstable();
    assert_eq!(game.resource::<LiveCarriers>().carried(), carriers);
    // Veil goes: the change keeps the value it last had, and refreshes no more, as its source
    // changes no more. Read with no source it would fall to 0.06 alone, 1 006 633 bits.
    let entity = game.unit_entity(source);
    game.despawn(entity);
    game.step();
    Release::check_copy(&mut game, &mut copy);
    assert_eq!(veil.values(&game, ward)[1], vamp);
    assert_eq!(game.resource::<LiveCarriers>().carried(), []);
    // At no stack it adds nothing, and is no live change that refreshes its unit every pass.
    let dual_path = Stats::modifier(&game, PackageIndex::MODE, "dual_path").unwrap();
    CarriedMut::with(&mut game, ward_entity, |carried| {
        carried.set_stacks(dual_path, Some(source), 0, Tick::new(0));
    })
    .unwrap();
    game.step();
    Release::check_copy(&mut game, &mut copy);
    assert_eq!(veil.values(&game, ward)[1], Num::ZERO);
    assert_eq!(game.resource::<LiveCarriers>().carried(), []);
}
