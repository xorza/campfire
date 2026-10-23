use std::num::NonZeroU8;

use bevy::math::Vec3;
use campfire_actions::{SlotKind, SlotKindData, SlotRanks};
use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_sim::IdAllocator;
use campfire_units::GroundPoint;
use campfire_values::{ActionId, Rank};

use super::*;
use crate::pointer::Pointed;

#[test]
fn the_ability_keys_cast_the_slots_of_the_kinds_whose_ranks_are_learned() {
    // A weapon of ranks from the spawn, then two learned basics and an ultimate: Q, W and E
    // take slots 1 to 3, whatever the weapon stands first.
    let kind = |name, ranks: Option<u8>| SlotKindData {
        name: DeclaredName::new(name).unwrap(),
        ranks: ranks.map(|count| SlotRanks::new(NonZeroU8::new(count).unwrap(), None).unwrap()),
    };
    let kinds = [
        kind("weapon", None),
        kind("basic", Some(2)),
        kind("ultimate", Some(1)),
    ];
    let kinds = SlotKinds::new(kinds).unwrap();
    let held = |kind| (ActionId::new(0), SlotKind::new(kind), Some(Rank::FIRST));
    let slots = ActionSlots::new([held(0), held(1), held(1), held(2)]);
    let abilities: Vec<u8> = OrdersPlugin::abilities(&slots, &kinds).collect();
    assert_eq!(abilities, [1, 2, 3]);
}

#[test]
fn a_key_aims_as_its_action_takes() {
    let unit = IdAllocator::default().allocate();
    let point = SimVec3::from((3, 0, -2));
    // An action of a point, or of a direction, takes the point; of none, or no action,
    // nothing.
    let aims = [
        (Aim::Point { clamp: false }, Takes::Point),
        (Aim::Direction, Takes::Point),
        (Aim::None, Takes::Nothing),
    ];
    for (aim, takes) in aims {
        assert_eq!(Takes::of(Some(&aim)), takes, "{aim:?}");
    }
    assert_eq!(Takes::of(None), Takes::Nothing);
    // With a unit and a point under the cursor: the unit, the point, or nothing; with
    // neither under it, nothing.
    for (takes, both) in [
        (Takes::Unit, ActionTarget::Unit(unit)),
        (Takes::Point, ActionTarget::Point(point)),
        (Takes::Nothing, ActionTarget::None),
    ] {
        assert_eq!(takes.target(Some(unit), Some(point)), both, "{takes:?}");
        assert_eq!(takes.target(None, None), ActionTarget::None, "{takes:?}");
    }
}

#[test]
fn a_right_click_no_module_took_attacks_a_hostile_unit_and_else_walks() {
    let mut ids = IdAllocator::default();
    let (north, south) = (Team::new(0), Team::new(1));
    let mut unit = |team| Pointed {
        id: ids.allocate(),
        team,
        owner: None,
    };
    let (enemy, friend, creep) = (unit(south), unit(north), unit(Team::new(2)));
    let point = Vec3::new(3.0, 1.0, -2.5);
    // To (3, −2.5): x = 3, z = −5 / 2, the height not sent.
    let (x, z) = (Num::int(3), Num::int(-5) / Num::int(2));
    let walk = Some(
        OrdersDirective::Move {
            to: GroundPoint { x, z },
        }
        .into(),
    );
    // North regards south as hostile, its own team as friendly, and the creeps' as neutral.
    let regard = |side: Option<Team>| {
        move |team: Team| {
            let side = side?;
            Some(if team == side {
                Relation::Friendly
            } else if team == Team::new(2) {
                Relation::Neutral
            } else {
                Relation::Hostile
            })
        }
    };
    let click = |under, side| OrdersPlugin::click_directive(ClickAt { under, point }, regard(side));
    assert_eq!(
        click(Some(enemy), Some(north)),
        Some(OrdersDirective::Attack { target: enemy.id }.into())
    );
    assert_eq!(click(Some(friend), Some(north)), walk);
    assert_eq!(click(None, Some(north)), walk);
    // A neutral unit is walked to, not attacked; a client of no side has no enemy.
    assert_eq!(click(Some(creep), Some(north)), walk);
    assert_eq!(click(Some(enemy), None), walk);
    // A ground point far off any map orders no walk.
    let far = ClickAt {
        under: None,
        point: Vec3::new(1e15, 0.0, 0.0),
    };
    assert_eq!(
        OrdersPlugin::click_directive(far, regard(Some(north))),
        None
    );
}
