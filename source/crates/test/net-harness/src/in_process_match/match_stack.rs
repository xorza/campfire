use std::sync::Arc;

use bevy_app::{App, First};
use bevy_ecs::world::{EntityWorldMut, World};
use bevy_state::app::StatesPlugin;
use bevy_time::TimePlugin;
use campfire_app_threads::OneThreadPlugins;
use campfire_host::internals::TickHashes;
use campfire_host::{Hosting, JoinRole, Os, Pace, ServerPin, SessionSigner, UnixClock};
use campfire_math::TickRate;
use campfire_net::{LocalPacePlugin, SimClientPlugin, SimServerPlugins};
use campfire_net_prediction::{PredictionSetup, RollbackMode};
use campfire_net_stepper::Stack;
use campfire_net_sync::{LocalTimelineSync, TimelineSync};
use campfire_package::ModePackages;
use campfire_protocol::internals::TestKey;
use campfire_values::TeamSet;

use crate::in_process_match::link_layout::{LinkLayout, LinkRows};
use crate::in_process_match::{InProcessMatch, InProcessSetup};

/// The apps of an `InProcessMatch`, as its stepper adds them: a server of the session's mode, at
/// its default rate, and each remote client the client of its joiner.
#[derive(Debug, Clone)]
pub(crate) struct MatchStack {
    pub(crate) packages: Arc<ModePackages>,
    /// The pause and the speed every app follows.
    pub(crate) pace: Arc<Pace>,
    /// The Unix time every app's clock reads.
    pub(crate) clock: UnixClock,
    /// When each player's client rolls its state back, by player.
    pub(crate) rollbacks: [RollbackMode; InProcessSetup::MAX_PLAYERS],
    pub(crate) links: LinkLayout,
    /// Who each remote client joins as, by its index.
    pub(crate) joiners: Vec<Joiner>,
}

/// Who a client joins as: a player, whose keys and rollback mode the setup gives, or the
/// `n`th spectator, with a key of its own.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Joiner {
    Player(usize),
    Spectator(usize),
}

impl Stack for MatchStack {
    /// The client of `client`'s joiner, at the packages' default rate, following the pace and
    /// reading the clock.
    fn add_client(&self, app: &mut App, client: usize, _: TickRate) {
        let tick_hz = self.packages.manifest().tick_hz.default();
        let (keys, role, rollback) = match self.joiners[client] {
            Joiner::Player(player) => (
                TestKey::player(u32::try_from(player).unwrap()),
                JoinRole::Play,
                self.rollbacks[player],
            ),
            Joiner::Spectator(spectator) => (
                TestKey::spectator(u32::try_from(spectator).unwrap()),
                JoinRole::Watch {
                    teams: TeamSet::ALL,
                },
                RollbackMode::Check,
            ),
        };
        let sim_client = SimClientPlugin {
            main: Arc::new(TestKey::client_signer(keys.main)),
            session: SessionSigner::new(TestKey::client_signer(keys.session)),
            server: ServerPin {
                key: TestKey::server().x_only_public_key().0,
                tls_key: InProcessMatch::tls_key(),
                tick_hz,
            },
            role,
            hosting: Hosting::Local,
            packages: Arc::clone(&self.packages),
            os: Os {
                clock: self.clock.clone(),
                entropy: TestKey::client_entropy(),
            },
            data: None,
            bot: None,
        };
        app.add_plugins((OneThreadPlugins, TimePlugin, StatesPlugin));
        app.add_plugins(sim_client);
        app.add_plugins(LocalPacePlugin {
            pace: Arc::clone(&self.pace),
            tick_rate: InProcessMatch::rate(&self.packages).rate(),
        });
        let world = app.world_mut();
        world.resource_mut::<PredictionSetup>().rollback_setup.mode = rollback;
    }

    /// The local server of the mode, which keeps the state hash after every tick, and lays its
    /// links out as the setup says.
    fn add_server(&self, app: &mut App, _: TickRate) {
        let manifest = self.packages.manifest();
        app.add_plugins(SimServerPlugins {
            tick_hz: manifest.tick_hz.default(),
            capabilities: manifest.capabilities,
            hosting: Hosting::Local,
        });
        app.add_plugins(LocalPacePlugin {
            pace: Arc::clone(&self.pace),
            tick_rate: InProcessMatch::rate(&self.packages).rate(),
        });
        app.init_resource::<TickHashes>();
        if self.links == LinkLayout::Reversed {
            app.init_resource::<LinkRows>();
            app.add_systems(First, LinkRows::lay_out);
        }
    }

    /// None: each end's plugins register the session's protocol.
    fn add_protocol(&self, _: &mut App) {}

    fn client_link(&self, link: &mut EntityWorldMut<'_>) {
        SimClientPlugin::insert_parts(link);
    }

    fn server_link(&self, _: &mut EntityWorldMut<'_>) {}

    fn is_ready(&self, world: &World) -> bool {
        world
            .get_resource::<LocalTimelineSync>()
            .is_some_and(TimelineSync::is_synced)
    }
}
