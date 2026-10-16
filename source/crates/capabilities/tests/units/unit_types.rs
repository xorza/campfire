use campfire_capabilities::Release;
use campfire_units::*;
use campfire_values::{DeclaredName, NameId, PackageIndex, Tag, TypeScope, UnitType};

#[test]
fn the_types_load_up_to_the_most_tags_and_types_a_match_holds() {
    let mut types = UnitTypes::new(Release::engine_tags());
    // The engine's 7 tags and 249 more fill the 256 a match holds.
    for at in Release::engine_tags().len()..Tag::LIMIT {
        types.declare(&format!("tag{at}")).unwrap();
    }
    assert_eq!(types.declare("one_more"), Err(TypesError::TooManyTags));
    // A full match still finds the tags it holds, and a type that names a new one fails.
    assert_eq!(types.declare("tag255").unwrap().index(), 255);
    let named = |name: &str| UnitTypeCore {
        tags: vec![DeclaredName::new(name).unwrap()],
        ..UnitTypeCore::default()
    };
    let load = |types: &mut UnitTypes, name: &str, data: &UnitTypeCore| {
        types.load(TypeScope::Mode, PackageIndex::MODE, name, data)
    };
    assert_eq!(
        load(&mut types, "tower", &named("one_more")),
        Err(TypesError::TooManyTags)
    );
    // 65,536 types fill a match, and one more fails. The names sort in the order they load.
    let plain = UnitTypeCore::default();
    for at in 0..UnitType::LIMIT {
        load(&mut types, &format!("type{at:05}"), &plain).unwrap();
    }
    assert_eq!(
        load(&mut types, "type65536", &named("tag255")),
        Err(TypesError::TooManyTypes)
    );
}
