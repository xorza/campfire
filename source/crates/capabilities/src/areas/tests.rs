use serde::Serialize;

use super::*;
use crate::units::engine_tag::EngineTag;
use crate::units::filter::Filter;
use crate::units::tag_set::TagSet;
use crate::units::unit_types::UnitTypes;
use crate::values::relation::Relation;

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
