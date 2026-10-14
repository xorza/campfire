use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};

use campfire_capabilities::Release;
use campfire_units::*;
use campfire_values::{Block, DeclaredName, Tag, TagSet, TypeScope};

#[test]
fn a_units_tags_take_their_properties_from_the_modes_data() {
    let mut types = UnitTypeBook::new(Release::engine_tags());
    let names = ["stunned", "slowed", "slow_immune", "true_sight"];
    let [stunned, slowed, slow_immune, sight] = names.map(|name| types.declare(name).unwrap());
    // The engine's tags hold the first places, and to declare one finds it.
    assert_eq!(
        [stunned, slowed, slow_immune, sight].map(Tag::index),
        [7, 8, 9, 10]
    );
    let release = Release::engine_tags();
    let engine: Vec<Tag> = release
        .iter()
        .map(|tag| types.declare(tag.name()).unwrap())
        .collect();
    let numbers: Vec<Tag> = release.iter().map(|tag| tag.tag()).collect();
    assert_eq!(engine, numbers);
    assert_eq!(
        engine.iter().map(|tag| tag.index()).collect::<Vec<_>>(),
        [0, 1, 2, 3, 4, 5, 6]
    );
    assert_eq!(types.declare("tower").unwrap().index(), 11);
    let tower = UnitTypeCore {
        tags: vec![DeclaredName::new("true_sight").unwrap()],
        params: BTreeMap::new(),
        state: BTreeMap::new(),
    };
    let tower = types.load(TypeScope::Mode, 0, "tower", &tower).unwrap();
    let data = |blocks: &[Block], detects, immune: &[&str]| TagData {
        blocks: blocks.to_vec(),
        hidden: false,
        detects,
        immune: immune
            .iter()
            .map(|&name| DeclaredName::new(name).unwrap())
            .collect(),
    };
    let stun = [Block::Move, Block::Attack, Block::Cast, Block::Use];
    let data = BTreeMap::from(
        [
            ("stunned", data(&stun, false, &[])),
            ("slow_immune", data(&[], false, &["slowed"])),
            ("true_sight", data(&[], true, &[])),
        ]
        .map(|(name, data)| (DeclaredName::new(name).unwrap(), data)),
    );
    let book = types.tag_book(&data);
    let set = |tags: &[Tag]| tags.iter().copied().collect::<TagSet>();

    // Its type's true sight detects; a stun blocks what the data names, and no more.
    let tags = book.unit_tags(tower, [set(&[stunned]), set(&[slowed])].into_iter());
    assert_eq!(tags.tags, set(&[stunned, slowed, sight]));
    assert!(tags.properties.detects() && !tags.properties.hidden());
    let blocked: Vec<_> = Block::ALL
        .into_iter()
        .filter(|&block| tags.properties.blocks(block))
        .collect();
    assert_eq!(blocked, stun);
    assert_eq!(tags.immune, TagSet::default());

    // Slow immunity holds a slow without its tag, in either order.
    for granted in [
        [set(&[slowed]), set(&[slow_immune])],
        [set(&[slow_immune]), set(&[slowed])],
    ] {
        let tags = book.unit_tags(tower, granted.into_iter());
        assert_eq!(tags.tags, set(&[sight, slow_immune]));
        assert_eq!(tags.immune, set(&[slowed]));
    }

    // The book took the types' tags: a later load, a later tag or a second book fails, as it
    // would be lost.
    let mut fails = |late: &mut dyn FnMut(&mut UnitTypeBook)| {
        panic::catch_unwind(AssertUnwindSafe(|| late(&mut types))).is_err()
    };
    assert!(fails(&mut |types| {
        types
            .load(TypeScope::Mode, 0, "late", &UnitTypeCore::default())
            .unwrap();
    }));
    assert!(fails(&mut |types| types.give_tag(tower, stunned)));
    assert!(fails(&mut |types| drop(types.tag_book(&data))));
}
