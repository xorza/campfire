use std::collections::BTreeMap;
use std::convert::Infallible;

use campfire_capabilities::{Match, Release};
use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_script::ParamProblem;
use campfire_script::ParamProblem::{Missing, Overflow, ScalingTime, Short, Time};
use campfire_stats::*;
use campfire_units::{ScriptBook, StatId, UnitTypes};
use campfire_values::{
    ActionId, ModifierId, NameId, Number, PackageIndex, Param, ParamRef, Rank, Ranked, Scalar,
    Scaling, Stat,
};

fn name(text: &str) -> DeclaredName {
    DeclaredName::new(text).unwrap()
}

fn param(text: &str) -> Number {
    Number::Param(ParamRef { param: name(text) })
}

#[test]
fn a_way_applies_a_modifier_only_when_it_gives_each_param_the_modifier_reads() {
    // The action's params: 2⁴⁰, past a number's 2³⁹; −5 ms; 100 and 200 ms at its 2 ranks;
    // and a scaling table of 2 ranks.
    let int = |value| Param::Ranked(Ranked::One(Scalar::Int(value)));
    let scaling = Param::Scaling(Scaling {
        base: Ranked::from([Num::ONE, Num::ONE]),
        per_level: Num::ZERO,
        bonus: BTreeMap::new(),
        ratios: BTreeMap::new(),
    });
    let per_rank = Ranked::from([Scalar::Int(100), Scalar::Int(200)]);
    let action = BTreeMap::from([
        (name("huge"), int(1 << 40)),
        (name("late"), int(-5)),
        (name("pace"), Param::Ranked(per_rank)),
        (name("power"), scaling),
    ]);
    // By name, each modifier reads one param, as a shield or as a duration; `own` declares
    // its param, of one rank.
    let shield = |read| ModifierData {
        shield: Some(param(read)),
        ..ModifierData::default()
    };
    let lasting = |read| ModifierData {
        duration_ms: Some(param(read)),
        ..ModifierData::default()
    };
    let own = ModifierData {
        params: BTreeMap::from([(name("own"), Param::Ranked(Ranked::from([Scalar::Int(1)])))]),
        ..shield("own")
    };
    let data = [
        ("absent", shield("absent")),
        ("huge", shield("huge")),
        ("late", lasting("late")),
        ("own", own),
        ("pace", lasting("pace")),
        ("power", lasting("power")),
        ("scaled", shield("power")),
    ]
    .map(|(id, data)| (name(id), data));
    let rate = Match::TEST_RATE;
    let mut book = ModifierBook::default();
    let types = UnitTypes::new(Release::engine_tags());
    let load = ModifierLoad {
        scripts: &ScriptBook::new(Release::hooks()),
        tags: types.tags(),
        stat: |_: &_| Ok(StatId::new(0)),
        rate,
    };
    let modifiers = data.each_ref().map(|(name, data)| PackageModifier {
        name,
        data,
        script: None,
    });
    book.load(load, PackageIndex::MODE, &modifiers).unwrap();
    let mut tables = ParamTables::default();
    let stat = |_: &Stat| Ok::<_, Infallible>(StatId::new(0));
    tables.push_action(&action, stat).unwrap();
    let second = BTreeMap::from([(name("pace"), int(300))]);
    tables.push_action(&second, stat).unwrap();
    for (_, data) in &data {
        tables.push_modifier(&data.params, stat).unwrap();
    }
    let params = ParamBook::new(tables, book.appliers());
    let ways = |action, rank| -> Vec<Option<ParamProblem>> {
        let rank = Rank::new(rank).unwrap();
        let check = |at| book.check_way(ModifierId::at(at), action, rank, &params, rate);
        (0..data.len()).map(|at| check(at).err()).collect()
    };
    // By the action at rank 2: no `absent`; 2⁴⁰ past a number; −5 ms no time; `own` has no
    // second rank; 200 ms holds; a time of a scaling table does not, and a shield of it does.
    let action = Some(ActionId::new(0));
    let by_action = [Missing, Overflow, Time, Short].map(Some);
    let by_action = [by_action.as_slice(), &[None, Some(ScalingTime), None]].concat();
    assert_eq!(ways(action, 2), by_action);
    // At rank 1 `own` holds too; at rank 3 no per-rank param has a value, the action's as
    // its own; with no action, only `own`, which declares its param, holds.
    assert_eq!(ways(action, 1)[3], None);
    let past = [Missing, Overflow, Time, Short, Short, Short, Short].map(Some);
    assert_eq!(ways(action, 3), past);
    let mut no_action = vec![Some(Missing); data.len()];
    no_action[3] = None;
    assert_eq!(ways(None, 1), no_action);
    // A second action declares only `pace`, 300 ms, at its first place where the first action
    // holds it at its third: by it, only `own` and `pace` hold.
    let mut by_second = vec![Some(Missing); data.len()];
    by_second[3] = None;
    by_second[4] = None;
    assert_eq!(ways(Some(ActionId::new(1)), 1), by_second);
    // A live param is the action's scaling table, at a rank it has: not 2⁴⁰, which is no
    // table, nor a place past its params, nor rank 3.
    let live = |at| LiveParam {
        origin: ParamOrigin::Action(ActionId::new(0)),
        at,
    };
    let [second, third] = [2, 3].map(|rank| Rank::new(rank).unwrap());
    assert!(params.holds_live(live(3), second));
    assert!(!params.holds_live(live(0), second));
    assert!(!params.holds_live(live(4), second));
    assert!(!params.holds_live(live(3), third));
}
