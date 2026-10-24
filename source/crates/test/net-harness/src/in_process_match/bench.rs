use std::time::Duration;

use campfire_bots::OrderScript;
use campfire_math::{Num, Vec3};
use campfire_measure::RunCost;
use campfire_net_emulator::LinkModel;
use campfire_net_prediction::RollbackMode;
use campfire_orders::OrdersDirective;
use campfire_sim::{Position, SimWorld};
use campfire_store::internals::TestDir;
use campfire_test_packages::TestMode;
use campfire_units::GroundPoint;

use crate::in_process_match::{CONNECT_FRAMES, End, InProcessMatch, InProcessSetup, StepCost};

/// A quarter meter a tick crosses the 10 m between the two targets in 40 ticks, so a new
/// order every 40 frames keeps the avatar walking and the server sending updates.
const LEG_FRAMES: u64 = 40;
/// The ticks of the lane 1v1 a run plays.
pub(crate) const MATCH_TICKS: u64 = 600;
/// The frames of a run of a walk: 10 legs.
pub(crate) const WALK_FRAMES: u64 = 10 * LEG_FRAMES;

/// How each player's client of the walking 3v3 rolls back: only on a misprediction, on
/// every confirmed update, and never.
const ROLLBACKS_3V3: [RollbackMode; 3] = [
    RollbackMode::Check,
    RollbackMode::Always,
    RollbackMode::Disabled,
];
/// The MOBA 3v3's first wave at its 30 Hz: 60 s of pick, then 60 s.
const FIRST_WAVE_3V3: u64 = 3600;
/// The pick of each slot of the MOBA 3v3, the players' then the bots': a hero each,
/// none twice, in tick 1, and the two spells in tick 2, as `Moba3v3`'s slots pick.
static PICKS_3V3: [&str; 6] = [
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

impl StepCost {
    /// What the step cost `end`.
    pub(crate) fn of(&self, end: End) -> Duration {
        match end {
            End::Server => self.server,
            End::Client(client) => self.clients[client],
        }
    }

    /// What the step cost the slowest client.
    pub(crate) fn slowest_client(&self) -> Duration {
        let slowest = self.clients.iter().copied().max();
        slowest.expect("a cost of a match with clients")
    }
}

impl InProcessMatch {
    /// A solo match whose client rolls back as `rollback` says, started, for `walk_run`.
    pub(crate) fn walking(rollback: RollbackMode) -> Self {
        let mut local = Self::new(InProcessSetup::solo(rollback, 1, Self::SEED_CHAIN));
        local.start_match();
        local
    }

    /// The MOBA 3v3 at its default rate, a server frame a tick, with a player in each
    /// of the first slots, whose clients roll back as `ROLLBACKS_3V3` says, and the
    /// server's bots in the others, every slot picking as `PICKS_3V3` says; played to its
    /// first wave, each client holding its hero, for `walk_run`.
    pub(crate) fn walking_3v3() -> Self {
        let players = ROLLBACKS_3V3.len();
        let mut rollbacks = [RollbackMode::Check; InProcessSetup::MAX_PLAYERS];
        rollbacks[..players].copy_from_slice(&ROLLBACKS_3V3);
        let setup = InProcessSetup::solo(RollbackMode::Check, 1, Self::SEED_CHAIN)
            .into_builder()
            .players(players)
            .rollbacks(rollbacks)
            .bots(&PICKS_3V3[players..])
            .build();
        let packages = TestMode::Moba3v3.packages();
        let mut local = Self::of_mode(setup, packages);
        local.open_match();
        for (client, pick) in PICKS_3V3[..players].iter().enumerate() {
            local.play(client, &OrderScript::parse(pick).expect("a pick reads"));
        }
        while local.next_tick(End::Server) < FIRST_WAVE_3V3 {
            local.step();
        }
        local.await_avatars(CONNECT_FRAMES);
        local
    }

    /// Where each client's avatar stands on the server, by client, into `places`.
    pub(crate) fn avatar_places(&self, places: &mut Vec<Vec3>) {
        places.clear();
        let world = self.stepper.server_world();
        for client in 0..self.stepper.client_count() {
            let avatar = world.entity_of(self.avatar(client));
            let position = world.get::<Position>(avatar.expect("the avatar exists"));
            places.push(position.expect("an avatar has a place").get());
        }
    }

    /// The cost of a run of `WALK_FRAMES` frames of a walk from frame `*frame` on, which it
    /// advances, each frame's what `cost` reads of its step's: every `LEG_FRAMES` frames, each
    /// client's avatar ordered to the target 5 m on the other side of its point in `around`,
    /// along z.
    pub(crate) fn walk_run(
        &mut self,
        frame: &mut u64,
        around: &[Vec3],
        cost: impl Fn(&StepCost) -> Duration,
    ) -> RunCost {
        debug_assert_eq!(around.len(), self.stepper.client_count());
        RunCost::of(WALK_FRAMES, || {
            if frame.is_multiple_of(LEG_FRAMES) {
                let side = if frame.is_multiple_of(2 * LEG_FRAMES) {
                    5
                } else {
                    -5
                };
                let side = Num::from_int(side).expect("a small integer");
                for (client, point) in around.iter().enumerate() {
                    let (x, z) = (point.x, point.z + side);
                    self.order(
                        client,
                        OrdersDirective::Move {
                            to: GroundPoint { x, z },
                        },
                    );
                }
            }
            self.step();
            *frame += 1;
            cost(&self.step_cost)
        })
    }

    /// The cost of `MATCH_TICKS` ticks of a new lane 1v1, each tick's what `cost` reads of its
    /// step's, as the match scenario plays it: the rollback of each avatar's death falls in the
    /// clients'. The server and the clients keep their data, in a test directory of its own, so
    /// the journal and the receipts write as a real match's do; it is removed after, as the match
    /// drops first.
    pub(crate) fn lane_1v1(cost: impl Fn(&StepCost) -> Duration) -> RunCost {
        let test_dir = TestDir::new();
        let mut local = Self::new(InProcessSetup::duo(LinkModel::perfect(), Self::SEED_CHAIN));
        local.keep_data(test_dir.path("data"));
        local.start_match();
        local.play_by_team(Self::SCENARIO_SCRIPTS);
        RunCost::of(MATCH_TICKS, || {
            local.step();
            cost(&local.step_cost)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_cost_reads_each_end_and_the_slowest_client() {
        // Client 0 took 3 µs, client 1 9 µs, and the server 5 µs: the slowest client is client 1.
        let micros = Duration::from_micros;
        let cost = StepCost {
            clients: vec![micros(3), micros(9)],
            server: micros(5),
        };
        let ends = [End::Server, End::Client(0), End::Client(1)];
        assert_eq!(ends.map(|end| cost.of(end)), [5, 3, 9].map(micros));
        assert_eq!(cost.slowest_client(), micros(9));
    }
}
