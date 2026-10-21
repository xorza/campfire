use std::num::NonZeroU8;

use campfire_actions::{SlotKind, SlotKindData, SlotRanks};
use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_sim::IdAllocator;
use campfire_values::{ActionId, Rank};

use super::*;

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
    // A ground point, exactly, its height among its axes: 0.1 in `f32` is 13,421,773 · 2⁻²⁷,
    // which rounds to the step 1,677,722 of 2⁻²⁴, not a millimeter's multiple.
    assert_eq!(
        position(Vec3::new(3.0, 0.0, -2.0)),
        Some(SimVec3::from((3, 0, -2)))
    );
    let tenth = position(Vec3::new(0.1, 0.0, 0.0)).unwrap();
    assert_eq!(tenth.x.to_bits(), 1_677_722);
    assert_eq!(position(Vec3::new(1e15, 0.0, 0.0)), None);
}

#[test]
fn a_cast_key_casts_at_its_target_and_with_ctrl_learns() {
    let target = ActionTarget::None;
    assert_eq!(
        OrdersPlugin::slot_directive(2, KeyUse::Cast, target),
        Directive::Orders(OrdersDirective::Slot { slot: 2, target })
    );
    assert_eq!(
        OrdersPlugin::slot_directive(2, KeyUse::Learn, target),
        Directive::Progression(ProgressionDirective::Learn { slot: 2 })
    );
}

#[test]
fn a_right_click_uses_what_the_avatar_may_use_rallies_producers_attacks_an_enemy_and_else_walks() {
    let mut ids = IdAllocator::default();
    let (north, south) = (Team::new(0), Team::new(1));
    let mut unit = |team| Pointed {
        id: ids.allocate(),
        team,
        owner: None,
    };
    let (enemy, friend) = (unit(south), unit(north));
    let point = Vec3::new(3.0, 1.0, -2.5);
    // To (3, −2.5): x = 3, z = −5 / 2, the height not sent.
    let (x, z) = (Num::int(3), Num::int(-5) / Num::int(2));
    let walk = Some(
        OrdersDirective::Move {
            to: GroundPoint { x, z },
        }
        .into(),
    );
    let shrine = unit(south);
    let usable = |id| id == shrine.id;
    let click_by = |selected, under, team, point| {
        let click = Click {
            under,
            team,
            point,
            selected,
        };
        OrdersPlugin::click_directive(click, usable)
    };
    let click = |under, team| click_by(Selected::Movers, under, team, point);
    assert_eq!(
        click(Some(enemy), Some(north)),
        Some(OrdersDirective::Attack { target: enemy.id }.into())
    );
    assert_eq!(click(Some(friend), Some(north)), walk);
    assert_eq!(click(None, Some(north)), walk);
    // A client of no team has no enemy.
    assert_eq!(click(Some(enemy), None), walk);
    // A unit the avatar may use is used, before an attack of an enemy or a walk: the
    // hostile shrine by either team's client.
    let used = Some(OrdersDirective::Use { target: shrine.id }.into());
    assert_eq!(click(Some(shrine), Some(north)), used);
    assert_eq!(click(Some(shrine), Some(south)), used);
    // Producers rally to the point, or to the unit under the cursor, an enemy's too.
    let rally = |target| {
        Some(
            ProductionDirective::Rally {
                target: Some(target),
            }
            .into(),
        )
    };
    let producers = |under| click_by(Selected::Producers, under, Some(north), point);
    assert_eq!(
        producers(None),
        rally(RallyTarget::Point(GroundPoint { x, z }))
    );
    assert_eq!(producers(Some(friend)), rally(RallyTarget::Unit(friend.id)));
    assert_eq!(producers(Some(enemy)), rally(RallyTarget::Unit(enemy.id)));
    // A ground point far off any map orders nothing, a walk or a rally.
    let far = Vec3::new(1e15, 0.0, 0.0);
    for selected in [Selected::Movers, Selected::Producers] {
        assert_eq!(click_by(selected, None, Some(north), far), None);
    }
}

#[test]
fn a_selection_of_producers_and_no_walker_rallies() {
    let (walker, producer, tower) = (
        Mover {
            walks: true,
            trains: false,
        },
        Mover {
            walks: false,
            trains: true,
        },
        Mover {
            walks: false,
            trains: false,
        },
    );
    // A barracks alone, or with a tower; a barracks with a ranger, a ranger alone, a tower
    // alone, and nothing.
    for (units, selected) in [
        (&[producer][..], Selected::Producers),
        (&[producer, tower], Selected::Producers),
        (&[producer, walker], Selected::Movers),
        (&[walker], Selected::Movers),
        (&[tower], Selected::Movers),
        (&[], Selected::Movers),
    ] {
        assert_eq!(Selected::of(units.iter().copied()), selected, "{units:?}");
    }
}
