use std::collections::BTreeMap;
use std::num::NonZeroU8;
use std::sync::Arc;

use bevy_ecs::system::{Res, SystemState};

use campfire_abilities::internals::MANA;
use campfire_abilities::*;
use campfire_actions::{
    ActionBook, ActionData, ActionError, ActionField, ActionProblem, ActionSlots, ActionTarget,
    Actions, RangeField, SlotKindBook, SlotKindData, SlotKinds, SlotRanks, TargetTest, Targeting,
    Targets,
};
use campfire_capabilities::internals::*;
use campfire_capabilities::{Directive, Match, Order};
use campfire_common::{DeclaredName, PlayerSlot, Tick};
use campfire_declarations::internals::BookLoad;
use campfire_math::{Num, Vec3};
use campfire_navigation::Navigation;
use campfire_orders::OrdersDirective;
use campfire_progression::{Points, ProgressionDirective, TrackSet};
use campfire_script::ScriptLimits;
use campfire_sim::Capability;
use campfire_sim::internals::SimWorldInternals;
use campfire_stats::{Level, ModifierClocks, ModifierData, Modifiers, Stats};
use campfire_units::internals::Spawn;
use campfire_units::{Dead, Destination, GroundPoint, MoveStep, NeededLevel, UnitTypeCore, Units};
use campfire_values::{
    ActionId, FilterData, PackageIndex, Param, ParamRef, Rank, Ranked, Scalar, Team, TimeTooLarge,
    TrackId, TypeScope, UnitTags,
};

use crate::abilities::{LASH_OUT, STRIKE, abilities_match, cost, int, lash_out, param};

#[test]
fn an_ability_loads_only_when_its_data_holds() {
    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    // Each load under its own name, as a package names each action once.
    let load = |game: &mut Match, name: &str, data: &ActionData, source: &str| {
        let script = Units::compile_hooked(game, source).unwrap();
        Actions::load(game, PackageIndex::MODE, name, data, Some(script), 5)
    };
    let mut uneven = lash_out();
    uneven.keys.cost = BTreeMap::from([(
        DeclaredName::new("mana").unwrap(),
        Ranked::from([int(35), int(40)]),
    )]);
    let mut aimed = lash_out();
    aimed.keys.targeting = Targeting::Direction;
    let mut forever = lash_out();
    forever.keys.cooldown_ms = Some(Ranked::One(int(i64::MAX)));
    let mut scaled = lash_out();
    scaled.keys.cooldown_ms = Some(Ranked::One(param("damage")));
    let mut negative = lash_out();
    negative.keys.cost = cost("mana", int(-1));
    let mut poolless = lash_out();
    poolless.keys.cost = cost("focus", int(1));
    let mut unknown = lash_out();
    unknown.keys.range = Some(Ranked::One(RangeField::Param(ParamRef {
        param: DeclaredName::new("reach").unwrap(),
    })));
    // The data's rules, which its load holds: two costs for five ranks, and the field that does
    // not hold at rank 1. A scaling param is the caster's value, which no cooldown may take.
    let refused = |problem| Err(ActionError::Problem(problem));
    assert_eq!(
        [lash_out().check_ranks(5), uneven.check_ranks(5)],
        [true, false]
    );
    assert_eq!(
        load(&mut game, "uneven", &uneven, LASH_OUT),
        refused(ActionProblem::RankCount(5))
    );
    for (at, (data, field)) in [
        (scaled, ActionField::Cooldown),
        (negative, ActionField::Cost),
        (poolless, ActionField::Cost),
        (unknown, ActionField::Range),
    ]
    .into_iter()
    .enumerate()
    {
        let loaded = load(&mut game, &format!("field_{at}"), &data, LASH_OUT);
        assert_eq!(loaded, refused(ActionProblem::Field(field)), "{field:?}");
    }
    // What only a match's rate decides: i64::MAX ms counts in no tick.
    let forever = load(&mut game, "forever", &forever, LASH_OUT);
    assert_eq!(forever, refused(ActionProblem::Time(TimeTooLarge)));
    assert_eq!(
        load(&mut game, "lash_out", &lash_out(), LASH_OUT),
        Ok(ActionId::new(0))
    );
    // A direction loads, as every targeting does; no cast can aim one yet.
    assert_eq!(
        load(&mut game, "aimed", &aimed, LASH_OUT),
        Ok(ActionId::new(1))
    );

    // A script may serve only the ability's modifiers: a cast of rank 1 in tick 0 then runs no
    // script, and spends 35 of 100 and its 10 000 ms, 300 ticks at 30 a second.
    let modifiers_only = "fn on_damage_taken(ctx, m, d) { }";
    let passive = load(&mut game, "passive", &lash_out(), modifiers_only).unwrap();
    let caster = Abilities::caster(&mut game, passive, 1);
    Abilities::cast(&mut game, caster, ActionTarget::None);
    assert_eq!(Abilities::failed_calls(&mut game), []);
    assert_eq!(Abilities::pool(&game, caster), 65);
    assert_eq!(Abilities::time(&game, caster).ready_at, Tick::new(300));
}

#[test]
fn a_capability_field_reads_its_param_at_each_rank() {
    // A cooldown of `{ param = "cd" }`, 1000 ms at rank 1 up to 3000 at rank 3: 30, 60 and 90
    // ticks at 30 a second, and a cost of `{ param = "price" }` mana, 7 at every rank.
    let mut data = Abilities::strike();
    data.keys.cooldown_ms = Some(Ranked::One(param("cd")));
    data.keys.cost = BTreeMap::from([(
        DeclaredName::new("mana").unwrap(),
        Ranked::from([int(5), param("price"), int(9)]),
    )]);
    let per_rank = |values: &[i64]| {
        Param::Ranked(Ranked::PerRank(
            values.iter().map(|&value| Scalar::Int(value)).collect(),
        ))
    };
    data.keys.params.insert(
        DeclaredName::new("cd").unwrap(),
        per_rank(&[1000, 2000, 3000]),
    );
    data.keys.params.insert(
        DeclaredName::new("price").unwrap(),
        Param::Ranked(Ranked::One(Scalar::Int(7))),
    );
    let mut game = abilities_match();
    let strike = Units::compile_hooked(&mut game, STRIKE).unwrap();
    let id = Actions::load(
        &mut game,
        PackageIndex::MODE,
        "strike",
        &data,
        Some(strike),
        3,
    )
    .unwrap();
    let book = game.resource::<ActionBook>();
    let ranks: Vec<_> = book
        .get(id)
        .unwrap()
        .ranks
        .iter()
        .map(|values| {
            (
                values.cooldown.get(),
                values.cost.get(MANA).to_int().expect("a whole cost"),
            )
        })
        .collect();
    assert_eq!(ranks, [(30, 5), (60, 7), (90, 9)]);
    // Three ranks of an array of 3 is the rank count; five is not.
    assert_eq!([data.check_ranks(3), data.check_ranks(5)], [true, false]);
}

#[test]
fn a_unit_target_is_one_its_filter_selects_tag_and_all() {
    let mut game = abilities_match();
    let mut load_type = |name: &str| {
        let data = UnitTypeCore::tagged(&[name]);
        Units::load_type(&mut game, TypeScope::Mode, name, &data)
    };
    let (hero, creep) = (load_type("avatar"), load_type("creep"));
    for unit_type in [hero, creep] {
        Abilities::give_combatant(&mut game, unit_type);
    }
    let mut heroes_only = Abilities::strike();
    heroes_only.keys.targeting = Targeting::Unit(FilterData::parse("enemies:avatar").unwrap());
    let strike = Abilities::load(&mut game, "strike", &heroes_only, STRIKE);
    let caster = Abilities::caster(&mut game, strike, 1);
    let enemy_creep = Spawn::of(creep, (3, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    let enemy_hero = Spawn::of(hero, (4, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    // The start's rule of a unit target, as a bot's search runs it: the hero, and not the creep.
    let mut state = SystemState::<(Res<'_, ActionBook>, Targets<'_, '_>)>::new(&mut game);
    let (book, targets) = state.get(&game).unwrap();
    let test = TargetTest {
        book: &book,
        targets: &targets,
        team: *game.part::<Team>(caster),
        tags: game.part::<UnitTags>(caster).tags,
    };
    let slots = game.part::<ActionSlots>(caster);
    assert!(slots.may_aim(&test, 0, enemy_hero) && !slots.may_aim(&test, 0, enemy_creep));
    // The creep is an enemy, but no hero: the cast goes nowhere. The hero takes 50.
    Abilities::cast(&mut game, caster, ActionTarget::Unit(enemy_creep));
    assert_eq!(Stats::health(&game, enemy_creep), 500);
    Abilities::cast(&mut game, caster, ActionTarget::Unit(enemy_hero));
    assert_eq!(Stats::health(&game, enemy_hero), 450);
}

#[test]
fn a_learn_order_spends_a_point_on_the_next_rank_its_level_allows() {
    let declared = [
        Capability::Stats,
        Capability::Progression,
        Capability::Combat,
        Capability::Navigation,
        Capability::Abilities,
        Capability::Orders,
    ];
    let mut game = Abilities::match_of(ScriptLimits::ROOMY, &declared);
    Abilities::load_stats(&mut game);
    // A guard whose shield is Lash Out's damage at its rank, which its passive holds: 75 at rank
    // 1, then 25 more a rank. Its 5 ranks need levels 1, 1, 3, 3 and 5.
    let guard = ModifierData {
        shield: Some(param("damage")),
        ..ModifierData::default()
    };
    Stats::load_modifier(&mut game, PackageIndex::MODE, "guard", &guard, None);
    let mut data = lash_out();
    data.keys.passive_modifier = Some(DeclaredName::new("guard").unwrap());
    let lash_out = Abilities::load(&mut game, "lash_out", &data, LASH_OUT);
    let levels = [1, 1, 3, 3, 5].map(|level| NeededLevel::new(level).unwrap());
    let ranks = SlotRanks::new(NonZeroU8::new(5).unwrap(), Some(levels.to_vec())).unwrap();
    let basic = SlotKindData {
        name: DeclaredName::new("basic").unwrap(),
        ranks: Some(ranks),
    };
    SlotKindBook(Arc::new(SlotKinds::new([basic]).unwrap())).load_into(&mut game);
    // Player 0's caster, at level 1 with 2 points, its spawn's and one more.
    let caster = Abilities::caster(&mut game, lash_out, 0);
    let track = TrackId::new(0);
    let mut points = Points::at_spawn(TrackSet::from([track]), Some(track)).unwrap();
    points.gain(1);
    let step = MoveStep::new(Num::ONE).unwrap();
    game.insert_parts(caster, (points, Navigation::walker(step)));
    let entity = game.unit_entity(caster);
    let id = Stats::modifier(&game, PackageIndex::MODE, "guard").unwrap();
    let send = |game: &mut Match, slot: u32, actions: &[Directive]| {
        let orders: Vec<_> = actions
            .iter()
            .map(|&action| Order::one(caster, action))
            .collect();
        let payload = Order::payload(&orders);
        game.step_with(&[(PlayerSlot::new(slot), &payload)]);
    };
    // Its rank, its points, and the shield its passive holds.
    let progress = |game: &Match| {
        let modifiers = game.get::<Modifiers>(entity).unwrap();
        let clocks = game.get::<ModifierClocks>(entity).unwrap();
        let shield = clocks
            .shield_of(modifiers, id, Some(caster))
            .map(|shield| shield.to_int().unwrap());
        let points = game.get::<Points>(entity).unwrap().get();
        (
            Rank::count(Abilities::slot(game, caster).rank),
            points,
            shield,
        )
    };
    let learn = Directive::from(ProgressionDirective::Learn { slot: 0 });

    // Player 1 does not control the caster: nothing. Player 0 orders a walk, which a learn in the
    // next tick does not stop: rank 1 for a point, its shield in the same tick.
    send(&mut game, 1, &[learn]);
    assert_eq!(progress(&game), (0, 2, None));
    let walk = Directive::from(OrdersDirective::Move {
        to: GroundPoint {
            x: Num::int(9),
            z: Num::ZERO,
        },
    });
    send(&mut game, 0, &[walk]);
    send(&mut game, 0, &[learn]);
    assert_eq!(progress(&game), (1, 1, Some(75)));
    let destination = game.get::<Destination>(entity).unwrap().get();
    assert_eq!(destination, Some(Vec3::from((9, 0, 0))));
    // In tick 3, a cast ordered before two learns: the first learns rank 2 with the last point,
    // the second finds none, and the cast starts at rank 2, after the learns of its tick, with
    // rank 2's cooldown of 9000 ms, 270 ticks.
    let cast = Directive::from(OrdersDirective::Slot {
        slot: 0,
        target: ActionTarget::None,
    });
    send(&mut game, 0, &[cast, learn, learn]);
    assert_eq!(progress(&game), (2, 0, Some(100)));
    assert_eq!(Abilities::time(&game, caster).ready_at, Tick::new(3 + 270));
    // Rank 3 needs level 3: refused at level 1 with 4 points, learned at level 3 though dead.
    game.part_mut::<Points>(caster).gain(4);
    send(&mut game, 0, &[learn]);
    assert_eq!(progress(&game), (2, 4, Some(100)));
    *game.part_mut::<Level>(caster) = Level::new(3).unwrap();
    game.insert_parts(caster, Dead);
    send(&mut game, 0, &[learn]);
    let (rank, points, _) = progress(&game);
    assert_eq!((rank, points), (3, 3));
    game.entity_mut(entity).remove::<Dead>();
    // At level 5, ranks 4 and 5; a third learn finds the last rank, and spends nothing.
    *game.part_mut::<Level>(caster) = Level::new(5).unwrap();
    send(&mut game, 0, &[learn, learn, learn]);
    assert_eq!(progress(&game), (5, 1, Some(175)));
    assert_eq!(Abilities::failed_calls(&mut game), []);
}
