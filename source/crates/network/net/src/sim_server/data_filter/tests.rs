use bevy_ecs::world::Mut;
use campfire_codec::Toml;
use campfire_progression::{Experience, Points};
use campfire_sim::{OnAny, SimComponent};
use campfire_states::SentOnChange;
use campfire_units::{AudienceAll, UnitState};
use campfire_values::TeamSet;
use campfire_visibility::{ModeVisibility, Visibility};
use serde::{Deserialize, Serialize};

use super::*;
use crate::sim_server::viewer::Watching;

/// A state type of `progression` that no scope holds.
#[derive(Component, Debug, Serialize, Deserialize)]
struct Stray;

impl SimComponent for Stray {
    const NAME: &'static str = "test.stray";
    type Host = OnAny;

    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Stray {
    const KIND: DataKind = DataKind::Progression;
    type Sending = SentOnChange;
}

/// The components of `S` as replicon registers them in `app`, as the text of their mask,
/// which replicon gives no other way to compare.
fn mask<S: FilterScope>(app: &mut App) -> String {
    let world = app.world_mut();
    world.init_resource::<ReplicationRegistry>();
    world.resource_scope(|world, mut registry: Mut<'_, ReplicationRegistry>| {
        let VisibilityScope::Components(mask) = S::visibility_scope(world, &mut registry) else {
            unreachable!("a components scope");
        };
        format!("{mask:?}")
    })
}

#[test]
fn each_filtered_kinds_scope_holds_the_types_the_lists_give_it() {
    // Progression's kind holds its experience and its points; that of a unit's script state
    // fields of `audience = "all"`, their one component alone.
    let mut app = App::new();
    let progression = mask::<KindScope<ProgressionKind>>(&mut app);
    assert_eq!(progression, mask::<(Experience, Points)>(&mut app));
    let all = mask::<KindScope<UnitStateAllKind>>(&mut app);
    assert_ne!(all, progression);
    assert_eq!(
        all,
        mask::<SingleComponent<UnitState<AudienceAll>>>(&mut app)
    );
    // Every kind is filtered once but the three the dispatch names as unfiltered.
    for kind in DataKind::ALL {
        let filters = FILTERED
            .iter()
            .filter(|&&filtered| filtered == kind)
            .count();
        let unfiltered = matches!(kind, DataKind::Unit | DataKind::Match | DataKind::Server);
        assert_eq!(filters, usize::from(!unfiltered), "{kind:?}");
    }
}

#[test]
fn a_kinds_filter_follows_its_entitys_owner_and_team() {
    // `Stray`, of `progression`: its filter holds the owner and team the entity has as it
    // gains it, each it changes to, and none of one once it loses it; a despawn changes
    // nothing.
    let mut app = App::new();
    app.insert_resource(MatchVisibility(Visibility::DEFAULT));
    DataFilter::<ProgressionKind>::keep(&mut app);
    DataFilter::<ProgressionKind>::required_by::<Stray>(&mut app);
    let world = app.world_mut();
    let held = |world: &World, entity| {
        world
            .get::<DataFilter<ProgressionKind>>(entity)
            .map(|filter| (filter.owner, filter.team))
    };
    let (seat, team) = (Seat::new, Team::new);
    let hero = world.spawn((Owner::new(seat(1)), team(0), Stray)).id();
    world.flush();
    assert_eq!(held(world, hero), Some((Some(seat(1)), Some(team(0)))));
    world.entity_mut(hero).insert(Owner::new(seat(2)));
    world.flush();
    assert_eq!(held(world, hero), Some((Some(seat(2)), Some(team(0)))));
    world.entity_mut(hero).insert(team(1));
    world.flush();
    assert_eq!(held(world, hero), Some((Some(seat(2)), Some(team(1)))));
    world.entity_mut(hero).remove::<Owner>();
    world.flush();
    assert_eq!(held(world, hero), Some((None, Some(team(1)))));
    world.entity_mut(hero).insert(Owner::new(seat(2)));
    world.entity_mut(hero).remove::<Team>();
    world.flush();
    assert_eq!(held(world, hero), Some((Some(seat(2)), None)));
    // A unit no player owns, as a creep; and one that holds no type of the kind.
    let creep = world.spawn((team(1), Stray)).id();
    let tower = world.spawn((Owner::new(seat(1)), team(0))).id();
    world.flush();
    assert_eq!(held(world, creep), Some((None, Some(team(1)))));
    assert_eq!(held(world, tower), None);
    // A player's own entity: its seat is its owner, through each change of its team.
    let own = world.spawn((Player::new(seat(3)), team(0), Stray)).id();
    world.flush();
    assert_eq!(held(world, own), Some((Some(seat(3)), Some(team(0)))));
    world.entity_mut(own).remove::<Team>();
    world.flush();
    assert_eq!(held(world, own), Some((Some(seat(3)), None)));
    world.entity_mut(own).insert((Owner::new(seat(4)), team(1)));
    world.entity_mut(own).remove::<Owner>();
    world.flush();
    assert_eq!(held(world, own), Some((Some(seat(3)), Some(team(1)))));
    world.entity_mut(hero).insert(Owner::new(seat(1)));
    world.flush();
    world.despawn(hero);
    world.flush();
    assert!(world.get_entity(hero).is_err());
}

/// Whether each of `viewers` holds what `filter` filters, as replicon asks it for `client`.
fn held<K: FilteredKind>(
    filter: &DataFilter<K>,
    viewers: &[Option<Viewer>],
    client: Entity,
) -> Vec<bool> {
    viewers
        .iter()
        .map(|viewer| filter.is_visible(client, viewer.as_ref()))
        .collect()
}

#[test]
fn only_the_kinds_audience_in_the_match_holds_a_filtered_kind() {
    let player = |seat, team| {
        Some(Viewer::Player {
            seat: Seat::new(seat),
            team: Team::new(team),
            out: false,
        })
    };
    let spectator = Some(Viewer::Spectator(Watching::Teams(TeamSet::ALL)));
    let client = Entity::PLACEHOLDER;
    let (owner, team) = (Owner::new(Seat::new(1)), Team::new(0));
    // The owner of team 0's unit, an ally, an enemy, a spectator, and a link with no seat.
    let viewers = [player(1, 0), player(0, 0), player(2, 1), spectator, None];
    // The defaults: prediction to the owner alone; a team field to its team; life to all.
    let defaults = MatchVisibility(Visibility::DEFAULT);
    let prediction = DataFilter::<PredictionKind>::new(Some(&owner), Some(&team), &defaults);
    let team_state = DataFilter::<UnitStateTeamKind>::new(Some(&owner), Some(&team), &defaults);
    let life = DataFilter::<LifeKind>::new(Some(&owner), Some(&team), &defaults);
    assert_eq!(
        held(&prediction, &viewers, client),
        [true, false, false, false, false]
    );
    assert_eq!(
        held(&team_state, &viewers, client),
        [true, true, false, false, false]
    );
    assert_eq!(
        held(&life, &viewers, client),
        [true, true, true, true, false]
    );

    // A mode that keeps life from enemies, and shows the inventory to spectators: the
    // owner and its allies hold life still.
    let mode: ModeVisibility = Toml::parse(
        r#"
life = ["spectators"]
inventory = ["spectators"]
"#,
    )
    .unwrap();
    let set = MatchVisibility(*mode.settings());
    let life = DataFilter::<LifeKind>::new(Some(&owner), Some(&team), &set);
    let inventory = DataFilter::<InventoryKind>::new(Some(&owner), Some(&team), &set);
    assert_eq!(
        held(&life, &viewers, client),
        [true, true, false, true, false]
    );
    assert_eq!(
        held(&inventory, &viewers, client),
        [true, false, false, true, false]
    );
    // A unit no player owns, as a creep: its team's kind to its team, and nothing of the
    // owner's to anyone.
    let creep = DataFilter::<PredictionKind>::new(None, Some(&team), &defaults);
    assert_eq!(
        held(&creep, &viewers, client),
        [false, false, false, false, false]
    );
    let creep = DataFilter::<UnitStateTeamKind>::new(None, Some(&team), &defaults);
    assert_eq!(
        held(&creep, &viewers, client),
        [true, true, false, false, false]
    );
}
