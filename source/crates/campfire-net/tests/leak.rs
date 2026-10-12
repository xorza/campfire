//! The leak test (design 14, D13): a scripted 3v3 in process, with spectators among its clients,
//! checks after each tick that each client holds each sent component on exactly the entities its
//! kind's audience admits, and each entity exactly where its team's, or its spectator's, view
//! holds it: once with the defaults, and once with every setting at its other extreme, through
//! the host's flags for those the mode leaves to it and the mode's `[visibility]` for the rest.
//! It reads the capabilities' lists of state types, so a wrong filter, a scope that lacks a type,
//! a dropped setting and a new type each fail by the type, the entity and the client.

use std::collections::BTreeMap;
use std::fmt::{Debug, Write};
use std::path::PathBuf;
use std::sync::Arc;

use bevy_ecs::component::{Component, ComponentId, Immutable, Mutable};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Allow, Has, With};
use bevy_ecs::world::World;
use campfire_capabilities::{
    DataKind, Dead, Declared, HostVisibility, Owner, Player, PlayerResources, Recipients, Release,
    Replication, SeenBy, StateTypes, Team, TeamSet, Visibility,
};
use campfire_common::{MapName, PlayerSlot};
use campfire_net::internals::{End, InProcessMatch, MatchSetup};
use campfire_net::{JoinState, OrderScript, TickHashes, WatchRequest};
use campfire_package::{ModePackages, PackageDir};
use campfire_sim::{EntityIndex, SimResource, StableId, Unpredicted};
use lightyear::prelude::{Replicate, Replicated, RollbackMode};

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

/// The pick of each slot, the players' then the bots': a hero each, none twice, in tick 1, and
/// the spells `haste` and `mend` in tick 2.
const PICKS: [&str; 6] = [
    "[[input]]\ntick = 1\nname = \"hero\"\nvalue = \"hero-cinder\"\n\
     [[input]]\ntick = 2\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
    "[[input]]\ntick = 1\nname = \"hero\"\nvalue = \"hero-gale\"\n\
     [[input]]\ntick = 2\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
    "[[input]]\ntick = 1\nname = \"hero\"\nvalue = \"hero-husk\"\n\
     [[input]]\ntick = 2\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
    "[[input]]\ntick = 1\nname = \"hero\"\nvalue = \"hero-kensho\"\n\
     [[input]]\ntick = 2\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
    "[[input]]\ntick = 1\nname = \"hero\"\nvalue = \"hero-rime\"\n\
     [[input]]\ntick = 2\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
    "[[input]]\ntick = 1\nname = \"hero\"\nvalue = \"hero-veil\"\n\
     [[input]]\ntick = 2\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
];

/// How a client views the match, as the test knows it, not as the server's links say.
#[derive(Debug, Clone, Copy)]
enum Role {
    Player {
        slot: PlayerSlot,
        team: Team,
    },
    /// A spectator of every unit, or of the views of `teams` together.
    Spectator {
        teams: Option<TeamSet>,
    },
}

impl Role {
    /// Whether it holds an entity its team, or the teams it watches, see as `seen` says: one with
    /// no `SeenBy` reaches every client, as the player and match entities do (design 14, D1).
    fn sees(self, seen: Option<&SeenBy>) -> bool {
        let Some(seen) = seen else {
            return true;
        };
        match self {
            Role::Player { team, .. } => seen.get().contains(team),
            Role::Spectator { teams: None } => true,
            Role::Spectator { teams: Some(teams) } => {
                seen.get().teams().any(|team| teams.contains(team))
            }
        }
    }

    /// Whether it receives `kind` on an entity of `owner` and `team`, in a session of
    /// `visibility`: the owner every kind but the server's, the others as the kind's audience
    /// holds them, each an ally of its own team and of an entity of none, an enemy of another, or
    /// a spectator (design 14, D1).
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
            Role::Player { slot, .. } if Some(slot) == owner => return true,
            Role::Player { team: own, .. } if team.is_some_and(|team| team != own) => {
                Recipients::ENEMIES
            }
            Role::Player { .. } => Recipients::ALLIES,
            Role::Spectator { .. } => Recipients::SPECTATORS,
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
    fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
        self.push::<C>();
    }

    fn on_insert<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
        self.push::<C>();
    }

    fn on_change<C: Replication + Component<Mutability = Mutable>>(&mut self, _: Declared) {
        self.push::<C>();
    }

    fn predicted<C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
        _: Declared,
    ) {
        self.push::<C>();
    }

    fn server<C: Replication>(&mut self, _: Declared) {
        self.push::<C>();
    }

    fn derived<C: Component>(&mut self) {}

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {
        self.push::<C>();
    }

    fn resource<R: SimResource>(&mut self) {}

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
    /// The kinds a client held on an entity it does not own, and those withheld from one that
    /// held the entity: what shows each audience was set as the run means.
    held_by_other: Vec<DataKind>,
    withheld: Vec<DataKind>,
}

/// The 3v3 with its pick and first wave a second each, and the heroes spawning 6 m apart in the
/// jungle south of the north base, 20 m and more from every structure and camp: a fight of the
/// heroes alone within the test's ticks; and `visibility` after its data, a `[visibility]` table
/// or nothing.
fn short_3v3(visibility: &str) -> ModePackages {
    let mut files = PackageDir::workspace_tree("test/moba");
    let edit = |files: &mut BTreeMap<PathBuf, Vec<u8>>, path: &str, from: &str, to: &str| {
        let path = PathBuf::from(path);
        let text = String::from_utf8(files[&path].clone()).unwrap();
        assert!(text.contains(from), "{from}");
        files.insert(path, text.replacen(from, to, 1).into_bytes());
    };
    let data = "modes/3v3/data/mode.toml";
    edit(&mut files, data, "pick_ms = 60000", "pick_ms = 1000");
    edit(
        &mut files,
        data,
        "first_wave_ms = 60000",
        "first_wave_ms = 1000",
    );
    let map = "modes/3v3/map/two_lanes/map.toml";
    for (team, from, to) in [("north", -60, -28), ("south", 60, -22)] {
        let marker = |z| format!("team = \"{team}\"\npos = [0, {z}]");
        edit(&mut files, map, &marker(from), &marker(to));
    }
    let path = PathBuf::from(data);
    let mut text = String::from_utf8(files[&path].clone()).unwrap();
    text.push_str(visibility);
    files.insert(path, text.into_bytes());
    let dir = PackageDir::in_memory(Arc::new(PackageDir::reindexed(files)), "modes/3v3");
    ModePackages::from_package_dir(&dir, &MapName::new("two_lanes").unwrap()).unwrap()
}

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
    ) -> LeakCheck {
        let mut setup = MatchSetup::solo(RollbackMode::Check, 1, InProcessMatch::SEED_CHAIN);
        setup.players = PLAYERS;
        setup.visibility = host;
        setup.max_spectators = u32::try_from(spectators.len()).unwrap();
        setup.bots = &PICKS[PLAYERS..];
        let mut local = InProcessMatch::of_mode(setup, packages);
        let hashes = local
            .server_mut()
            .world_mut()
            .remove_resource::<TickHashes>();
        assert!(hashes.is_some());
        local.open_match();
        for (client, pick) in PICKS[..PLAYERS].iter().enumerate() {
            local.play(client, OrderScript::parse(pick).unwrap());
        }
        local.await_avatars(JOIN_FRAMES);
        let host = HostVisibility {
            settings: host.iter().map(|text| text.parse().unwrap()).collect(),
            max_spectators: setup.max_spectators,
        };
        let visibility = local.packages().data().visibility.session(&host).unwrap();
        let mut roles: Vec<Role> = (0..PLAYERS)
            .map(|client| Role::Player {
                slot: PlayerSlot::new(u32::try_from(client).unwrap()),
                team: local.team(client),
            })
            .collect();
        for &teams in spectators {
            let spectator = local.add_spectator();
            let watching = |local: &InProcessMatch| {
                let state = local.client(spectator).world().resource::<JoinState>();
                state.watching().is_some()
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
                world.insert_resource(WatchRequest(teams));
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
        LeakCheck {
            local,
            roles,
            visibility,
            lists,
            server_ids,
            client_ids,
            failures: Vec::new(),
            held_by_other: Vec::new(),
            withheld: Vec::new(),
        }
    }

    /// Plays `ticks` ticks, each client's frame then the server's, and checks every client after
    /// its frame, before the server's: a client then holds what the server's last frame sent.
    fn play(&mut self, ticks: u64) {
        let frames = self.local.setup().server_frames;
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
                    .map(|owner| owner.slot())
                    .or(player.map(|player| player.slot()));
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
                (With<Replicated>, Allow<Unpredicted>),
            >();
            let held: BTreeMap<StableId, (Entity, bool)> = query
                .iter(world)
                .map(|(entity, &id, unpredicted)| (id, (entity, unpredicted)))
                .collect();
            let world = self.local.client(client).world();
            let server = self.local.server().world();
            for (&id, shown) in &shown {
                let sees = role.sees(shown.seen.as_ref());
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
                    let owns =
                        matches!(role, Role::Player { slot, .. } if Some(slot) == shown.owner);
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

    /// Asserts no check failed, naming the first failures.
    fn assert_no_leak(&self) {
        let first: Vec<&String> = self.failures.iter().take(20).collect();
        assert!(
            self.failures.is_empty(),
            "{} failures, the first: {first:#?}",
            self.failures.len()
        );
    }

    /// Orders the north's first two heroes to strike the south's in `STRIKE` ticks from now.
    fn strike(&mut self) {
        let tick = self.local.next_tick(End::Server) + STRIKE;
        let target = self.local.avatar(3).get();
        for client in 0..2 {
            let script = format!("[[order]]\ntick = {tick}\nattack = {target}\n");
            self.local
                .play(client, OrderScript::parse(&script).unwrap());
        }
    }

    /// Whether the server holds the south hero dead.
    fn south_hero_dead(&self) -> bool {
        let world = self.local.server().world();
        let hero = world.resource::<EntityIndex>().get(self.local.avatar(3));
        hero.is_some_and(|hero| world.entity(hero).contains::<Dead>())
    }

    /// Whether `client` holds `PlayerResources` on the entity of player `slot`.
    fn holds_resources_of(&self, client: usize, slot: u32) -> bool {
        let world = self.local.client(client).world();
        let player = world.iter_entities().find(|entity| {
            entity
                .get::<Player>()
                .is_some_and(|player| player.slot() == PlayerSlot::new(slot))
        });
        player.is_some_and(|player| player.contains::<PlayerResources>())
    }
}

mod scripted_match {
    use super::*;

    #[test]
    fn each_client_holds_exactly_what_each_kinds_audience_admits_with_either_extreme() {
        // The defaults: the spectators see every unit, through any fog.
        let mut defaults = LeakCheck::start(short_3v3(""), &[], &[None]);
        defaults.strike();
        let mut died = false;
        for _ in 0..MATCH_TICKS {
            defaults.play(1);
            died |= defaults.south_hero_dead();
        }
        defaults.assert_no_leak();
        assert!(died, "the south's hero fell in the defaults' run");
        // A player's money is its own, and its unit's life every client's that holds it.
        assert!(defaults.withheld.contains(&DataKind::Resources));
        assert!(defaults.held_by_other.contains(&DataKind::Life));
        assert!(!defaults.holds_resources_of(1, 0) && !defaults.holds_resources_of(4, 0));

        // The other extreme: the spectators watch the teams' views, one every team's and one the
        // south's alone, as the host's flags say; each kind's audience flips as the mode's
        // `[visibility]` says.
        let south = TeamSet::from_iter([Team::new(1)]);
        let mut extreme = LeakCheck::start(
            short_3v3(&other_extreme()),
            &["spectators=teams", "out=spectators"],
            &[None, Some(south)],
        );
        extreme.strike();
        let mut died = false;
        for _ in 0..MATCH_TICKS {
            extreme.play(1);
            died |= extreme.south_hero_dead();
        }
        extreme.assert_no_leak();
        assert!(died, "the south's hero fell in the extreme's run");
        // A player's allies and the spectators hold its money; no one but the owner its unit's
        // life.
        assert!(extreme.held_by_other.contains(&DataKind::Resources));
        assert!(extreme.withheld.contains(&DataKind::Life));
        assert!(extreme.holds_resources_of(1, 0) && extreme.holds_resources_of(4, 0));
    }
}
