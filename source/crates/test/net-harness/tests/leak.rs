//! The leak test (10-replication-audiences.md, A leak test over every type and two settings): a
//! scripted 3v3 in process, with spectators among its clients, checks after each tick that each
//! client holds each sent component on exactly the entities its kind's audience admits, and each
//! entity exactly where its team's, or its spectator's, view holds it: once with the defaults, and
//! once with every setting at its other extreme, through the host's flags for those the mode leaves
//! to it and the mode's `[visibility]` for the rest. It reads the capabilities' lists of state
//! types, so a wrong filter, a scope that lacks a type, a dropped setting and a new type each fail
//! by the type, the entity and the client.

use std::collections::BTreeMap;
use std::fmt::{Debug, Write};

use bevy_ecs::component::{Component, ComponentId, Mutable};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Allow, Has, With};
use bevy_ecs::world::World;
use bevy_replicon::prelude::Remote;
use campfire_bots::OrderScript;
use campfire_capabilities::Release;
use campfire_common::PlayerSlot;
use campfire_host::JoinState;
use campfire_host::internals::TickHashes;
use campfire_net_harness::{End, InProcessMatch, InProcessSetup};
use campfire_net_prediction::RollbackMode;
use campfire_net_replication::Replicate;
use campfire_package::ModePackages;
use campfire_players::{HeldOut, Player, PlayerResources, PlayersWorld};
use campfire_sim::{EntityIndex, SimResource, SimWorld, StableId, Unpredicted};
use campfire_states::{Replication, StateTypes};
use campfire_test_packages::Edit;
use campfire_units::{Dead, Owner, SeenBy};
use campfire_values::{Team, TeamSet};
use campfire_visibility::{DataKind, Recipients};
use campfire_visibility::{HostVisibility, OutView, SpectatorFog, Visibility};

use crate::short_3v3::{PICKS, short_3v3};

/// The players with a client, in slots 0 to 3: three of the north and one of the south, so each
/// unit of a player has an owner's, an ally's and an enemy's client; the bots take slots 4 and 5.
const PLAYERS: usize = 4;
/// Frames a spectator may take to link, answer the offer and watch, and the avatars to come.
const JOIN_FRAMES: usize = 300;
/// The ticks the match plays after the avatars spawned, each checked: the north's first two
/// heroes fell the south's in the jungle, and the waves walk their lanes.
const MATCH_TICKS: u64 = 700;
/// Frames for a spectator's choice of teams to reach the server, and what it changes to reach its
/// client.
const CHOICE_FRAMES: usize = 3;
/// The tick the north's first two heroes strike the south's.
const STRIKE: u64 = 60;

/// How a client views the match, as the test knows it, not as the server's links say.
#[derive(Debug, Clone, Copy)]
enum Role {
    /// The player of `slot`, of `team`; `out` while it is out of play in a session whose players
    /// out of play see the other teams as spectators do (10-replication-audiences.md, A player out
    /// of play sees what its team sees, or what a spectator sees).
    Player {
        player_slot: PlayerSlot,
        team: Team,
        out: bool,
    },
    /// A spectator of every unit, or of the views of `teams` together.
    Spectator { teams: Option<TeamSet> },
}

impl Role {
    /// Whether it holds an entity its team, or the teams it watches, see as `seen` says, in a
    /// session of `visibility`: one with no `SeenBy`, or of which no Vision stage spoke, reaches
    /// every client, as the player and match entities do (10-replication-audiences.md, Four
    /// recipients); a player out of play also sees what the spectators' fog shows, every unit or
    /// every team's view.
    fn sees(self, seen: Option<&SeenBy>, visibility: &Visibility) -> bool {
        let Some(seen) = seen.and_then(|seen| seen.get()) else {
            return true;
        };
        let any = seen.iter().next().is_some();
        match self {
            Self::Player { team, out, .. } => {
                seen.contains(team)
                    || (out && (visibility.spectators() == SpectatorFog::All || any))
            }
            Self::Spectator { teams: None } => true,
            Self::Spectator { teams: Some(teams) } => seen.iter().any(|team| teams.contains(team)),
        }
    }

    /// Whether it receives `kind` on an entity of `owner` and `team`, in a session of `visibility`:
    /// the owner every kind but the server's, the others as the kind's audience holds them, each an
    /// ally of its own team and of an entity of none, an enemy of another, or a spectator
    /// (10-replication-audiences.md, Four recipients); a player out of play a spectator of another
    /// team's entity.
    fn receives(
        self,
        kind: DataKind,
        owner: Option<PlayerSlot>,
        team: Option<Team>,
        visibility: &Visibility,
    ) -> bool {
        if !kind.is_sent() {
            return false;
        }
        let recipient = match self {
            Self::Player { player_slot, .. } if Some(player_slot) == owner => return true,
            Self::Player { team: own, out, .. } if team.is_some_and(|team| team != own) => {
                if out {
                    Recipients::SPECTATORS
                } else {
                    Recipients::ENEMIES
                }
            }
            Self::Player { .. } => Recipients::ALLIES,
            Self::Spectator { .. } => Recipients::SPECTATORS,
        };
        visibility.audience(kind).contains(recipient)
    }
}

/// A state type of the lists, its kind, and how a world registers it.
#[derive(Debug, Clone, Copy)]
struct Listed {
    name: &'static str,
    kind: DataKind,
    register: fn(&mut World) -> ComponentId,
}

fn register<C: Component>(world: &mut World) -> ComponentId {
    world.register_component::<C>()
}

/// Every state type a match of the 3v3's capabilities holds, sent or the server's: the derived
/// ones, which no client receives as state, aside.
#[derive(Debug, Default)]
struct Lists(Vec<Listed>);

impl Lists {
    fn push<C: Replication>(&mut self) {
        self.0.push(Listed {
            name: C::NAME,
            kind: C::KIND,
            register: register::<C>,
        });
    }
}

impl StateTypes for Lists {
    fn component<C: Replication>(&mut self) {
        self.push::<C>();
    }

    fn derived<C: Component>(&mut self) {}

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {
        self.push::<C>();
    }

    fn resource<R: SimResource + Default>(&mut self) {}

    fn sim_component<C: Replication>(&mut self) {
        self.push::<C>();
    }
}

/// A replicated entity of the server, as the check reads it.
#[derive(Debug)]
struct Shown {
    entity: Entity,
    owner: Option<PlayerSlot>,
    team: Option<Team>,
    seen: Option<SeenBy>,
}

/// The match, its clients' roles, the lists' types by world, and what the checks found.
#[derive(Debug)]
struct LeakCheck {
    local: InProcessMatch,
    roles: Vec<Role>,
    visibility: Visibility,
    lists: Vec<Listed>,
    /// Each type's id in the server's world, in the order of `lists`.
    server_ids: Vec<ComponentId>,
    /// Each type's id in each client's world, by client, in the order of `lists`.
    client_ids: Vec<Vec<ComponentId>>,
    /// Each failure, as the client, the entity and the type name it.
    failures: Vec<String>,
    /// Whether each player's client held an entity its team did not see, as one out of play does
    /// with `out = "spectators"`.
    beyond_team: [bool; PLAYERS],
    /// The kinds a client held on an entity it does not own, and those withheld from one that
    /// held the entity: what shows each audience was set as the run means.
    held_by_other: Vec<DataKind>,
    withheld: Vec<DataKind>,
}

/// Gale's player, of the north, out of play for 3 s from the pick's end, alive.
const HELD_OUT: [(&str, Edit<'_>); 3] = [
    (
        "modes/3v3/data/mode.toml",
        Edit::Replace(
            "timers = [\"income\", \"pick_end\", \"wave\"]",
            "timers = [\"back_in\", \"income\", \"pick_end\", \"wave\"]",
        ),
    ),
    (
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(
            "        ctx.state.phase = \"play\";\n",
            "        ctx.state.phase = \"play\";\n        ctx.set_out(ctx.players[1], true);\n        \
             ctx.after(\"back_in\", 3000);\n",
        ),
    ),
    (
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(
            "    } else if name == \"wave\" {",
            "    } else if name == \"back_in\" {\n        ctx.set_out(ctx.players[1], false);\n    \
             } else if name == \"wave\" {",
        ),
    ),
];

/// A `[visibility]` that sets every kind a mode may set to its other extreme: each recipient the
/// mode may set that the default lacks, and none it holds.
fn other_extreme() -> String {
    let mut table = String::from("\n[visibility]\n");
    for kind in DataKind::ALL {
        if kind.settable() == Recipients::NONE {
            continue;
        }
        let flipped = kind.settable().without(kind.audience());
        let names: Vec<String> = flipped.names().map(|name| format!("\"{name}\"")).collect();
        writeln!(table, "{} = [{}]", kind.name(), names.join(", ")).unwrap();
    }
    table
}

impl LeakCheck {
    /// The 3v3 of `packages`, the host giving `host` and admitting a spectator for each of
    /// `spectators`, which watches every unit, or the teams it names; started, each avatar
    /// spawned, and each spectator watching.
    fn start(
        packages: ModePackages,
        host: &'static [&'static str],
        spectators: &[Option<TeamSet>],
    ) -> Self {
        let setup = InProcessSetup::solo(RollbackMode::Check, 1, InProcessMatch::SEED_CHAIN)
            .into_builder()
            .players(PLAYERS)
            .visibility(host)
            .max_spectators(u32::try_from(spectators.len()).unwrap())
            .bots(&PICKS[PLAYERS..])
            .build();
        let mut local = InProcessMatch::of_mode(setup, packages);
        let hashes = local
            .server_mut()
            .world_mut()
            .remove_resource::<TickHashes>();
        assert!(hashes.is_some());
        local.open_match();
        for (client, pick) in PICKS[..PLAYERS].iter().enumerate() {
            local.play(client, &OrderScript::parse(pick).unwrap());
        }
        local.await_avatars(JOIN_FRAMES);
        let host = HostVisibility {
            settings: host.iter().map(|text| text.parse().unwrap()).collect(),
            max_spectators: Some(local.setup().max_spectators()),
        };
        let visibility = local.packages().data().visibility.session(&host).unwrap();
        let mut roles: Vec<Role> = (0..PLAYERS)
            .map(|client| Role::Player {
                player_slot: PlayerSlot::new(u32::try_from(client).unwrap()),
                team: local.team(client),
                out: false,
            })
            .collect();
        for &teams in spectators {
            let spectator = local.add_spectator();
            let watching = |local: &InProcessMatch| {
                let state = local.client(spectator).world().resource::<JoinState>();
                state.watching_clock().is_some()
            };
            for _ in 0..JOIN_FRAMES {
                if watching(&local) {
                    break;
                }
                local.step();
            }
            assert!(watching(&local), "the spectator watches");
            if let Some(teams) = teams {
                let world = local.client_mut(spectator).world_mut();
                world.resource_mut::<JoinState>().choose_teams(teams);
            }
            roles.push(Role::Spectator { teams });
        }
        // The server takes each spectator's choice of teams in its next frame, and its client
        // drops what the teams it left see in the frame after.
        for _ in 0..CHOICE_FRAMES {
            local.step();
        }
        let mut lists = Lists::default();
        let capabilities = local.packages().manifest().capabilities;
        Release::state_types(capabilities, &mut lists);
        let lists = lists.0;
        let server = local.server_mut().world_mut();
        let server_ids = lists
            .iter()
            .map(|listed| (listed.register)(server))
            .collect();
        let client_ids = (0..roles.len())
            .map(|client| {
                let world = local.client_mut(client).world_mut();
                lists
                    .iter()
                    .map(|listed| (listed.register)(world))
                    .collect()
            })
            .collect();
        Self {
            local,
            roles,
            visibility,
            lists,
            server_ids,
            client_ids,
            failures: Vec::new(),
            beyond_team: [false; PLAYERS],
            held_by_other: Vec::new(),
            withheld: Vec::new(),
        }
    }

    /// Plays `ticks` ticks, each client's frame then the server's, and checks every client after
    /// its frame, before the server's: a client then holds what the server's last frame sent.
    fn play(&mut self, ticks: u64) {
        let frames = self.local.setup().server_frames();
        for _ in 0..ticks {
            for client in 0..self.roles.len() {
                self.local.client_frame(client);
            }
            self.check();
            for _ in 0..frames {
                self.local.server_frame();
            }
        }
    }

    /// Checks what each client holds against what the server's entities and the session's
    /// visibility admit it, and notes each failure.
    fn check(&mut self) {
        let tick = self.local.next_tick(End::Server);
        self.mark_out();
        let server = self.local.server_mut().world_mut();
        let mut query = server.query_filtered::<(
            Entity,
            &StableId,
            Option<&Owner>,
            Option<&Player>,
            Option<&Team>,
            Option<&SeenBy>,
        ), With<Replicate>>();
        let shown: BTreeMap<StableId, Shown> = query
            .iter(server)
            .map(|(entity, &id, owner, player, team, seen)| {
                let owner = owner
                    .map(|owner| owner.player_slot())
                    .or_else(|| player.map(|player| player.player_slot()));
                let shown = Shown {
                    entity,
                    owner,
                    team: team.copied(),
                    seen: seen.copied(),
                };
                (id, shown)
            })
            .collect();
        for (client, &role) in self.roles.iter().enumerate() {
            let world = self.local.client_mut(client).world_mut();
            let mut query = world.query_filtered::<
                (Entity, &StableId, Has<Unpredicted>),
                (With<Remote>, Allow<Unpredicted>),
            >();
            let held: BTreeMap<StableId, (Entity, bool)> = query
                .iter(world)
                .map(|(entity, &id, unpredicted)| (id, (entity, unpredicted)))
                .collect();
            let world = self.local.client(client).world();
            let server = self.local.server().world();
            for (&id, shown) in &shown {
                let sees = role.sees(shown.seen.as_ref(), &self.visibility);
                if let Role::Player { team, .. } = role
                    && sees
                    && shown
                        .seen
                        .and_then(SeenBy::get)
                        .is_some_and(|seen| !seen.contains(team))
                {
                    self.beyond_team[client] = true;
                }
                let Some(&(entity, unpredicted)) = held.get(&id) else {
                    if sees {
                        self.failures
                            .push(format!("tick {tick}: client {client} lacks {id:?}"));
                    }
                    continue;
                };
                if !sees {
                    self.failures
                        .push(format!("tick {tick}: client {client} holds {id:?}, unseen"));
                    continue;
                }
                // A unit its client predicts is its own, which receives every sent kind; its
                // prediction runs ahead of the server, and inserts and removes on its own clock.
                if !unpredicted {
                    continue;
                }
                let at_server = server.entity(shown.entity);
                let at_client = world.entity(entity);
                for (at, listed) in self.lists.iter().enumerate() {
                    let on_server = at_server.contains_id(self.server_ids[at]);
                    let on_client = at_client.contains_id(self.client_ids[client][at]);
                    let receives =
                        role.receives(listed.kind, shown.owner, shown.team, &self.visibility);
                    let owns = matches!(role, Role::Player { player_slot, .. } if Some(player_slot) == shown.owner);
                    if on_server && !owns {
                        let noted = if receives {
                            &mut self.held_by_other
                        } else {
                            &mut self.withheld
                        };
                        if !noted.contains(&listed.kind) {
                            noted.push(listed.kind);
                        }
                    }
                    if on_client != (on_server && receives) {
                        self.failures.push(format!(
                            "tick {tick}: client {client} {} {} on {id:?}, which the server {}",
                            if on_client { "holds" } else { "lacks" },
                            listed.name,
                            if on_server { "holds" } else { "lacks" },
                        ));
                    }
                }
            }
            for id in held.keys().filter(|id| !shown.contains_key(id)) {
                self.failures.push(format!(
                    "tick {tick}: client {client} holds {id:?}, which the server does not show"
                ));
            }
        }
    }

    /// Marks each player out of play while its hero is dead or the script holds it out, in a
    /// session whose players out of play see as spectators (10-replication-audiences.md, A player
    /// out of play sees what its team sees, or what a spectator sees): the test's own reading of
    /// the server's state, not the server's `OutOfPlay`.
    fn mark_out(&mut self) {
        let sees_as_spectator = self.visibility.out() == OutView::Spectators;
        for client in 0..PLAYERS {
            let world = self.local.server().world();
            let hero = world
                .resource::<EntityIndex>()
                .get(self.local.avatar(client));
            let dead = hero.is_some_and(|hero| world.entity(hero).contains::<Dead>());
            let slot = PlayerSlot::new(u32::try_from(client).unwrap());
            let player = world.player_entity(slot).unwrap();
            let held = world.entity(player).contains::<HeldOut>();
            if let Role::Player { out, .. } = &mut self.roles[client] {
                *out = (dead || held) && sees_as_spectator;
            }
        }
    }

    /// Asserts no check failed, naming the first failures.
    fn assert_no_leak(&self) {
        let first: Vec<&String> = self.failures.iter().take(20).collect();
        assert!(
            self.failures.is_empty(),
            "{} failures, the first: {first:#?}",
            self.failures.len()
        );
    }

    /// Orders the north's first two heroes to strike the south's, the one fighter, in `STRIKE`
    /// ticks from now.
    fn strike(&mut self) {
        let tick = self.local.next_tick(End::Server) + STRIKE;
        for client in 0..2 {
            let script = format!(
                "[[order]]\ntick = {tick}\ndirective.orders.attack.target = \"enemies:avatar:fighter\"\n"
            );
            self.local
                .play(client, &OrderScript::parse(&script).unwrap());
        }
    }

    /// Whether the server holds the south hero dead.
    fn is_south_hero_dead(&self) -> bool {
        let world = self.local.server().world();
        let hero = world.entity_ref_of(self.local.avatar(3));
        hero.is_some_and(|hero| hero.contains::<Dead>())
    }

    /// Whether `client` holds `PlayerResources` on the entity of player `player_slot`.
    fn holds_resources_of(&self, client: usize, player_slot: u32) -> bool {
        let world = self.local.client(client).world();
        let player = world.iter_entities().find(|entity| {
            entity
                .get::<Player>()
                .is_some_and(|player| player.player_slot() == PlayerSlot::new(player_slot))
        });
        player.is_some_and(|player| player.contains::<PlayerResources>())
    }
}

mod scripted_match {
    use super::*;

    #[test]
    fn each_client_holds_exactly_what_each_kinds_audience_admits_with_either_extreme() {
        // The defaults: the spectators see every unit, through any fog.
        let mut defaults = LeakCheck::start(short_3v3(&HELD_OUT, ""), &[], &[None]);
        defaults.strike();
        let mut died = false;
        for _ in 0..MATCH_TICKS {
            defaults.play(1);
            died |= defaults.is_south_hero_dead();
        }
        defaults.assert_no_leak();
        assert!(died, "the south's hero fell in the defaults' run");
        // A player's money and its team's cells are its own, and its unit's life every client's
        // that holds it.
        assert!(defaults.withheld.contains(&DataKind::Resources));
        assert!(defaults.withheld.contains(&DataKind::Sight));
        assert!(defaults.held_by_other.contains(&DataKind::Life));
        assert!(!defaults.holds_resources_of(1, 0) && !defaults.holds_resources_of(4, 0));

        // The other extreme: the spectators watch the teams' views, one every team's and one the
        // south's alone, as the host's flags say; each kind's audience flips as the mode's
        // `[visibility]` says.
        let south = TeamSet::from([Team::new(1)]);
        let mut extreme = LeakCheck::start(
            short_3v3(&HELD_OUT, &other_extreme()),
            &["spectators=teams", "out=spectators"],
            &[None, Some(south)],
        );
        extreme.strike();
        let mut died = false;
        for _ in 0..MATCH_TICKS {
            extreme.play(1);
            died |= extreme.is_south_hero_dead();
        }
        extreme.assert_no_leak();
        assert!(died, "the south's hero fell in the extreme's run");
        // A player's allies and the spectators hold its money; no one but the owner its unit's
        // life.
        assert!(extreme.held_by_other.contains(&DataKind::Resources));
        assert!(extreme.withheld.contains(&DataKind::Life));
        assert!(extreme.holds_resources_of(1, 0) && extreme.holds_resources_of(4, 0));
        // Out of play, the south's dead hero's player and Gale's, whom the script held out alive,
        // held what their teams did not see; in the defaults, which keep a player out of play to
        // its team's view, no player did.
        assert_eq!(extreme.beyond_team, [false, true, false, true]);
        assert_eq!(defaults.beyond_team, [false; PLAYERS]);
    }
}
