//! Riders over the network: a rider's team alone receives it, hidden.

use bevy_ecs::entity_disabling::Disabled;
use campfire_harness::MatchUnits;
use campfire_interaction::Riding;
use campfire_math::Num;
use campfire_net_harness::{InProcessMatch, InProcessSetup};
use campfire_orders::OrdersDirective;
use campfire_sim::{Position, SimWorld};
use campfire_test_packages::{Edit, PackageTree, TestMode};
use campfire_units::{GroundPoint, Owner, SeenBy};
use campfire_values::{Team, TeamSet};

#[test]
fn a_rider_leaves_its_enemies_clients_and_stays_hidden_on_its_allies() {
    // A solo match of the lane mode, with interaction: the west player's client holds its hero and
    // the creeps it sees walk; here a creep of each team rides, as an entry leaves it, its team
    // alone seeing it (04-capabilities/interaction.md, A rider's team holds it).
    let mut tree = PackageTree::of(TestMode::Lane);
    tree.edit(
        "modes/lane/manifest.toml",
        Edit::Replace("\"progression\"]", "\"progression\", \"interaction\"]"),
    );
    let mut local = InProcessMatch::of_mode(InProcessSetup::SOLO, tree.packages());
    local.start_match();
    // The hero walks east until its client holds a unit of the east, as the east tower comes
    // into its sight.
    let east_at = GroundPoint {
        x: Num::int(4),
        z: Num::int(-2),
    };
    local.order(0, OrdersDirective::Move { to: east_at });
    let (west, east) = (Team::new(0), Team::new(1));
    let held = |local: &InProcessMatch, team: Team| {
        let world = local.server().world();
        let client = local.client(0).world();
        MatchUnits::of(world)
            .all()
            .find(|(id, unit)| {
                unit.get::<Team>() == Some(&team)
                    && !unit.contains::<Owner>()
                    && unit.contains::<Position>()
                    && client.entity_of(*id).is_some()
            })
            .map(|(id, _)| id)
    };
    for _ in 0..200 {
        if held(&local, east).is_some() {
            break;
        }
        local.step();
    }
    let ally = held(&local, west).unwrap();
    let enemy = held(&local, east).unwrap();
    // The creeps ride in the hero, and the hero, which its player's client predicts, in the west
    // creep: the network reads the mark alone.
    let hero = local.avatar(0);
    for (unit, team, container) in [(ally, west, hero), (enemy, east, hero), (hero, west, ally)] {
        let world = local.server_mut().world_mut();
        let entity = world.entity_of(unit).unwrap();
        world
            .entity_mut(entity)
            .insert((Riding::new(container), SeenBy::new(TeamSet::of(team))));
    }
    for _ in 0..10 {
        local.step();
    }
    let client = local.client(0).world();
    let held = client.entity_of(ally).unwrap();
    assert!(client.entity(held).contains::<Disabled>());
    assert_eq!(
        client.get::<Riding>(held).map(|riding| riding.container()),
        Some(hero)
    );
    assert_eq!(client.entity_of(enemy), None);
    let hero = client.entity_of(hero).unwrap();
    assert!(client.entity(hero).contains::<Disabled>());
}
