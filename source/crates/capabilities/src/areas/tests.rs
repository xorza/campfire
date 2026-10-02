use serde::Serialize;

use super::*;
use crate::actions::action_book;
use crate::areas::area_data::{AreaData, AreaInside};
use crate::capability_set::test_match::TestMatch;
use crate::units::Units;
use crate::units::engine_tag::EngineTag;
use crate::units::filter::Filter;
use crate::units::tag_set::TagSet;
use crate::units::type_scope::TypeScope;
use crate::units::unit_type_data::UnitTypeData;
use crate::units::unit_types::UnitTypes;
use crate::values::relation::Relation;
use campfire_sim::Capability;

#[test]
fn an_area_that_triggers_after_it_ends_fails_to_decode() {
    #[derive(Serialize)]
    struct Fields {
        source: u64,
        action: u32,
        rank: u8,
        aimed: Option<u64>,
        triggers_at: Option<u64>,
        ends_at: u64,
    }
    let decode = |triggers_at: Option<u64>, ends_at| {
        let fields = Fields {
            source: 1,
            action: 0,
            rank: 1,
            aimed: None,
            triggers_at,
            ends_at,
        };
        let bytes = postcard::to_allocvec(&fields).unwrap();
        postcard::from_bytes::<Area>(&bytes)
    };
    let area = decode(Some(5), 9).unwrap();
    assert_eq!(
        (area.triggers_at(), area.ends_at()),
        (Some(Tick::new(5)), Tick::new(9))
    );
    assert!(decode(Some(5), 5).is_ok());
    assert!(decode(None, 9).is_ok());
    assert!(decode(Some(6), 5).is_err());
}

#[test]
fn a_filter_selects_a_delivery_unit_only_when_it_names_its_tag() {
    let types = UnitTypes::default();
    let [projectile, area] = [EngineTag::Projectile, EngineTag::Area].map(EngineTag::tag);
    let parse = |text| Filter::parse(text, &types).unwrap();
    // A relation alone is the filter of its name.
    assert_eq!(Filter::of_relation(Relation::Enemies), parse("enemies"));
    assert_eq!(Filter::of_relation(Relation::All), parse("all"));
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
        let selects = units.map(|tags| parse(filter).selects(Attitude::Hostile, tags));
        assert_eq!(selects, selected, "{filter}");
    }
}

#[test]
fn an_area_reads_only_with_a_radius_that_is_not_negative() {
    let read = |text: &str| toml::from_str::<AreaData>(text);
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
        rank: 1,
    };
    let area = Area::new(by, None, Some(Tick::new(5)), Tick::new(9)).unwrap();
    let id = sim.spawn(Position::ORIGIN, (Team::new(0), field, area));
    let mut restored = TestMatch::client(&declared);
    loads(&mut restored);
    sim.restore_into(&mut restored);
    assert_eq!(restored.get::<Area>(id), sim.get::<Area>(id));
}
