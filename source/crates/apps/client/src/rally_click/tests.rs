use bevy::math::Vec3;
use campfire_math::Num;
use campfire_sim::IdAllocator;
use campfire_units::GroundPoint;
use campfire_values::Team;

use super::*;
use crate::pointer::Pointed;

#[test]
fn producers_rally_to_the_unit_under_the_cursor_or_the_point() {
    let friend = Pointed {
        id: IdAllocator::default().allocate(),
        team: Team::new(0),
        owner: None,
    };
    let point = Vec3::new(3.0, 1.0, -2.5);
    let rally = |target| {
        Some(
            ProductionDirective::Rally {
                target: Some(target),
            }
            .into(),
        )
    };
    // To (3, −2.5): x = 3, z = −5 / 2; at a unit, any team's; none far off any map.
    let (x, z) = (Num::int(3), Num::int(-5) / Num::int(2));
    let at = |under, point| RallyClickPlugin::rally_directive(ClickAt { under, point });
    assert_eq!(
        at(None, point),
        rally(RallyTarget::Point(GroundPoint { x, z }))
    );
    assert_eq!(at(Some(friend), point), rally(RallyTarget::Unit(friend.id)));
    assert_eq!(at(None, Vec3::new(1e15, 0.0, 0.0)), None);
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
