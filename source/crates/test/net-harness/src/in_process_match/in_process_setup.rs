use std::time::Duration;

use campfire_host::SessionTimes;
use campfire_mode::{PlayersData, SaveBy, SavesData};
use campfire_net::LobbyStart;
use campfire_net_emulator::LinkModel;
use campfire_net_prediction::RollbackMode;
use campfire_protocol::SeedChain;

use crate::in_process_match::InProcessMatch;
use crate::in_process_match::link_layout::LinkLayout;

/// What an `InProcessMatch` runs: how many players join, and how.
#[derive(Debug, Clone)]
pub struct InProcessSetup {
    /// The players, a client each, in the slots from 0: one or two in the lane mode, a slot a
    /// side, and at most `MAX_PLAYERS`.
    pub(crate) players: usize,
    /// How the server's links lie in its tables, which no result of the match may follow.
    pub(crate) links: LinkLayout,
    /// When each player's client rolls its state back, by player; those past the players go
    /// unused.
    pub(crate) rollbacks: [RollbackMode; Self::MAX_PLAYERS],
    /// Server frames a tick: a real server runs frames faster than ticks.
    pub(crate) server_frames: u32,
    /// How each link carries packets, both ways, on the stack's emulator.
    pub(crate) link: LinkModel,
    /// The stepper's seed, from which each link's draws follow.
    pub(crate) seed: u64,
    /// The seed chain the server commits to.
    pub(crate) seed_chain: SeedChain,
    /// How long the server waits for a player whose link fails, and restores a stopped session.
    pub(crate) times: SessionTimes,
    /// The lane mode's `[players]`.
    pub(crate) rules: PlayersData,
    /// The lane mode's `[saves]`.
    pub(crate) saves: SavesData,
    /// The order script of the server's bot in each slot after the players', in slot order.
    pub(crate) bots: &'static [&'static str],
    /// The name of the mode's bot that plays each slot after those of `bots`, in slot order.
    pub(crate) mode_bots: &'static [&'static str],
    /// The script of the server's bot in a slot its player left, when it plays one.
    pub(crate) takeover: Option<&'static str>,
    /// The host's visibility settings, as `--visibility` gives them.
    pub(crate) visibility: &'static [&'static str],
    /// The most spectators the session admits.
    pub(crate) max_spectators: u32,
    /// Whom the lobby waits for: its players, or a watcher too, of a plan that may hold bots
    /// alone.
    pub(crate) start: LobbyStart,
}

/// What builds an `InProcessSetup`: each field as its setter gives it, the rest as the setup it
/// starts from gives them; `build` checks the setup.
#[derive(Debug, Clone)]
pub struct InProcessSetupBuilder(InProcessSetup);

impl InProcessSetupBuilder {
    #[must_use]
    pub const fn players(mut self, players: usize) -> Self {
        self.0.players = players;
        self
    }

    #[must_use]
    pub const fn links(mut self, links: LinkLayout) -> Self {
        self.0.links = links;
        self
    }

    #[must_use]
    pub const fn rollbacks(
        mut self,
        rollbacks: [RollbackMode; InProcessSetup::MAX_PLAYERS],
    ) -> Self {
        self.0.rollbacks = rollbacks;
        self
    }

    #[must_use]
    pub const fn server_frames(mut self, server_frames: u32) -> Self {
        self.0.server_frames = server_frames;
        self
    }

    #[must_use]
    pub const fn link(mut self, link: LinkModel) -> Self {
        self.0.link = link;
        self
    }

    #[must_use]
    pub const fn seed(mut self, seed: u64) -> Self {
        self.0.seed = seed;
        self
    }

    #[must_use]
    pub fn seed_chain(mut self, seed_chain: SeedChain) -> Self {
        self.0.seed_chain = seed_chain;
        self
    }

    #[must_use]
    pub const fn times(mut self, times: SessionTimes) -> Self {
        self.0.times = times;
        self
    }

    #[must_use]
    pub const fn rules(mut self, rules: PlayersData) -> Self {
        self.0.rules = rules;
        self
    }

    #[must_use]
    pub const fn saves(mut self, saves: SavesData) -> Self {
        self.0.saves = saves;
        self
    }

    #[must_use]
    pub const fn bots(mut self, bots: &'static [&'static str]) -> Self {
        self.0.bots = bots;
        self
    }

    #[must_use]
    pub const fn mode_bots(mut self, mode_bots: &'static [&'static str]) -> Self {
        self.0.mode_bots = mode_bots;
        self
    }

    #[must_use]
    pub const fn takeover(mut self, takeover: Option<&'static str>) -> Self {
        self.0.takeover = takeover;
        self
    }

    #[must_use]
    pub const fn visibility(mut self, visibility: &'static [&'static str]) -> Self {
        self.0.visibility = visibility;
        self
    }

    #[must_use]
    pub const fn max_spectators(mut self, max_spectators: u32) -> Self {
        self.0.max_spectators = max_spectators;
        self
    }

    #[must_use]
    pub const fn start(mut self, start: LobbyStart) -> Self {
        self.0.start = start;
        self
    }

    /// The setup; panics on one of more players than `MAX_PLAYERS`, or of a server that runs no
    /// frame a tick.
    pub fn build(self) -> InProcessSetup {
        let Self(setup) = self;
        assert!(
            setup.players <= InProcessSetup::MAX_PLAYERS,
            "a setup holds at most `MAX_PLAYERS` players"
        );
        assert!(
            setup.server_frames > 0,
            "the server runs a frame a tick at least"
        );
        setup
    }
}

impl InProcessSetup {
    /// The most players a setup holds: the MOBA 3v3's six.
    pub const MAX_PLAYERS: usize = 6;

    /// One player through a perfect link, whose client rolls back only on a misprediction, with
    /// a server that runs a frame a tick.
    pub const SOLO: Self = Self::solo(RollbackMode::Check, 1, InProcessMatch::SEED_CHAIN);

    /// A link on which each packet waits 3 to 5 ticks of the lane mode's 30 Hz, 100 to 167 ms,
    /// and none is lost.
    pub const DELAYED_LINK: LinkModel = LinkModel::perfect()
        .with_latency(Duration::from_nanos(133_333_333))
        .with_jitter(Duration::from_nanos(33_333_333));

    /// The player slots of the match: its players', its order scripts' bots' and the mode's bots'.
    pub const fn player_slots(&self) -> usize {
        self.players + self.bots.len() + self.mode_bots.len()
    }
    /// One player, through perfect links.
    pub const fn solo(rollback: RollbackMode, server_frames: u32, seed_chain: SeedChain) -> Self {
        Self {
            players: 1,
            links: LinkLayout::InOrder,
            rollbacks: [rollback; Self::MAX_PLAYERS],
            server_frames,
            link: LinkModel::perfect(),
            seed: 0,
            seed_chain,
            times: SessionTimes::DEFAULT,
            rules: PlayersData::DEFAULT,
            saves: SavesData {
                by: SaveBy::Player,
                autosave_ms: None,
            },
            bots: &[],
            mode_bots: &[],
            takeover: None,
            visibility: &[],
            max_spectators: 0,
            start: LobbyStart::Players,
        }
    }

    /// Two players through `link`, whose clients roll back only on a misprediction, with a
    /// server that runs 3 frames a tick.
    pub fn duo(link: LinkModel, seed_chain: SeedChain) -> Self {
        Self::solo(RollbackMode::Check, 3, seed_chain)
            .into_builder()
            .players(2)
            .link(link)
            .build()
    }

    /// A builder that starts from this setup.
    pub const fn into_builder(self) -> InProcessSetupBuilder {
        InProcessSetupBuilder(self)
    }

    /// Server frames a tick.
    pub const fn server_frames(&self) -> u32 {
        self.server_frames
    }

    /// How each link carries packets.
    pub const fn link(&self) -> &LinkModel {
        &self.link
    }

    /// The most spectators the session admits.
    pub const fn max_spectators(&self) -> u32 {
        self.max_spectators
    }
}
