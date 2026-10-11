use campfire_common::{Tick, Ticks, Toml};
use campfire_math::Num;
use campfire_sim::SimComponent;

use super::*;
use crate::actions::action_book;
use crate::actions::launch_id::LaunchId;
use crate::areas::area_data::{AreaData, AreaInside};
use crate::capability_set::test_match::TestMatch;
use crate::stats::Stats;
use crate::stats::modifier_data::ModifierData;
use crate::units::Units;
use crate::units::engine_tag::EngineTag;
use crate::units::filter::Filter;
use crate::units::tag_set::TagSet;
use crate::units::team::Team;
use crate::units::type_scope::TypeScope;
use crate::units::unit_type_data::UnitTypeData;
use crate::units::unit_types::UnitTypes;
use crate::values::declared_name::DeclaredName;
use crate::values::number::{Number, ParamRef};
use crate::values::rank::Rank;
use crate::values::relation::Relation;
use crate::values::relation_set::RelationSet;

#[test]
fn a_filter_selects_a_delivery_unit_only_when_it_names_its_tag() {
    let types = UnitTypes::default();
    let [projectile, area] = [EngineTag::Projectile, EngineTag::Area].map(EngineTag::tag);
    let parse = |text| Filter::parse(text, &types).unwrap();
    // A relation alone is the filter of its name.
    assert_eq!(Filter::of_relations(RelationSet::Enemies), parse("enemies"));
    assert_eq!(Filter::of_relations(RelationSet::All), parse("all"));
    let units = [
        TagSet::default(),
        TagSet::of([projectile]),
        TagSet::of([area]),
    ];
    for (filter, selected) in [
        ("enemies", [true, false, false]),
        ("enemies:area", [false, false, true]),
        ("enemies:projectile", [false, true, false]),
        ("enemies:!area", [true, false, false]),
    ] {
        let selects = units.map(|tags| parse(filter).selects(Relation::Hostile, tags));
        assert_eq!(selects, selected, "{filter}");
    }
}

#[test]
fn an_area_reads_only_with_a_radius_that_is_not_negative() {
    let read = |text: &str| Toml::parse::<AreaData>(text);
    assert_eq!(
        read("radius = 0").unwrap(),
        AreaData {
            radius: Num::ZERO,
            delay_ms: 0,
            duration_ms: 0,
            affects: None,
            inside: AreaInside::default(),
        }
    );
    for refused in ["radius = -1", "radius = \"-0.5\""] {
        let message = read(refused).unwrap_err().message().to_owned();
        assert!(
            message.starts_with("an area's radius is not negative"),
            "{refused}"
        );
    }
}

#[test]
fn an_area_is_state_and_restores() {
    // A match of combat and areas with an area type and a train for its action, the same in both.
    let loads = |sim: &mut TestMatch| {
        let data = UnitTypeData::default();
        let field = Units::load_type(&mut sim.world, TypeScope::Mode, "field", &data);
        let area = AreaData {
            radius: Num::int(2),
            delay_ms: 0,
            duration_ms: 0,
            affects: None,
            inside: AreaInside::default(),
        };
        Areas::load_type(&mut sim.world, field, 0, &area);
        let action = action_book::internals::train(&mut sim.world, field, Ticks::ZERO, None);
        (field, action)
    };
    let declared = [Capability::Stats, Capability::Combat, Capability::Areas];
    let mut sim = TestMatch::client(&declared);
    let (field, action) = loads(&mut sim);
    let source = sim.spawn(Position::ORIGIN, Team::new(0));
    let by = Delivering {
        source,
        action,
        rank: Rank::FIRST,
        start: None,
        launch: None,
    };
    let area = Area::new(by, None, Tick::new(9));
    let trigger = AreaTrigger::at(Tick::new(5));
    let id = sim.spawn(Position::ORIGIN, (Team::new(0), field, area, trigger));
    let mut restored = TestMatch::client(&declared);
    loads(&mut restored);
    sim.restore_into(&mut restored);
    assert_eq!(restored.get::<Area>(id), sim.get::<Area>(id));
    assert_eq!(restored.get::<AreaTrigger>(id), &trigger);
    // It triggers by its end, at tick 9 at the latest; a unit that is no area has no trigger.
    let entity = sim.entity(id);
    let triggers = |at| AreaTrigger::at(Tick::new(at)).check(&sim.world, entity);
    assert_eq!([5, 9, 10].map(triggers), [true, true, false]);
    let unit = sim.spawn(Position::ORIGIN, Team::new(0));
    assert!(!trigger.check(&sim.world, sim.entity(unit)));
    // Its times are at most the limit: a tick past it fails to decode.
    let past = Tick::new(Tick::LIMIT.get() + 1);
    assert!(TestMatch::decodes(&Area::new(by, None, Tick::LIMIT)));
    assert!(!TestMatch::decodes(&Area::new(by, None, past)));
    assert!(TestMatch::decodes(&AreaTrigger::at(Tick::LIMIT)));
    assert!(!TestMatch::decodes(&AreaTrigger::at(past)));
    // An area of a launch the match did not load has no lists to run.
    let launched = Delivering {
        launch: Some(LaunchId::nth(0)),
        ..by
    };
    let area = Area::new(launched, None, Tick::LIMIT);
    assert!(!area.check(&sim.world, entity));
    // An area that holds Rally on allies, which reads its action's `ward`: the train, which
    // declares none, cannot have delivered it, as Rally would fail as an ally takes it.
    let ward = DeclaredName::new("ward").unwrap();
    let rally = ModifierData {
        shield: Some(Number::Param(ParamRef { param: ward })),
        ..ModifierData::default()
    };
    Stats::load_modifier(&mut sim.world, 0, "rally", &rally, None);
    let data = UnitTypeData::default();
    let warding = Units::load_type(&mut sim.world, TypeScope::Mode, "warding", &data);
    let inside = AreaInside {
        allies: Some(DeclaredName::new("rally").unwrap()),
        ..AreaInside::default()
    };
    let area = AreaData {
        radius: Num::int(2),
        delay_ms: 0,
        duration_ms: 0,
        affects: None,
        inside,
    };
    Areas::load_type(&mut sim.world, warding, 0, &area);
    let warded = sim.spawn(Position::ORIGIN, (Team::new(0), warding));
    let area = Area::new(by, None, Tick::LIMIT);
    assert!(area.check(&sim.world, entity));
    assert!(!area.check(&sim.world, sim.entity(warded)));
}
