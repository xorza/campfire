use std::time::Duration;

use campfire_bots::OrderScript;
use campfire_math::{Num, Vec3};
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
/// The ticks of the lane 1v1 a worst case plays.
const MATCH_TICKS: u64 = 600;

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
    /// Adds each end's cost in `step` to its own.
    fn add(&mut self, step: &Self) {
        self.clients.resize(step.clients.len(), Duration::ZERO);
        for (own, &spent) in self.clients.iter_mut().zip(&step.clients) {
            *own += spent;
        }
        self.server += step.server;
    }

    /// Keeps each end's worse of its own and its cost in `step`.
    fn keep_worst(&mut self, step: &Self) {
        self.clients.resize(step.clients.len(), Duration::ZERO);
        for (own, &spent) in self.clients.iter_mut().zip(&step.clients) {
            *own = (*own).max(spent);
        }
        self.server = self.server.max(step.server);
    }

    /// The worst of the clients' costs.
    pub(crate) fn worst_client(&self) -> Duration {
        let worst = self.clients.iter().copied().max();
        worst.expect("a cost of a match with clients")
    }
}

impl InProcessMatch {
    /// A solo match whose client rolls back as `rollback` says, started, for `walk_steps`.
    pub(crate) fn walking(rollback: RollbackMode) -> Self {
        let mut local = Self::new(InProcessSetup::solo(rollback, 1, Self::SEED_CHAIN));
        local.start_match();
        local
    }

    /// The MOBA 3v3 at its default rate, a server frame a tick, with a player in each
    /// of the first slots, whose clients roll back as `ROLLBACKS_3V3` says, and the
    /// server's bots in the others, every slot picking as `PICKS_3V3` says; played to its
    /// first wave, each client holding its hero, for `walk_steps`.
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

    /// `steps` frames of a walk from frame `*frame` on, which it advances: every
    /// `LEG_FRAMES` frames, each client's avatar ordered to the target 5 m on the other side
    /// of its point in `around`, along z; each end's frames' cost added to `spent`.
    pub(crate) fn walk_steps(
        &mut self,
        frame: &mut u64,
        steps: u64,
        around: &[Vec3],
        spent: &mut StepCost,
    ) {
        debug_assert_eq!(around.len(), self.stepper.client_count());
        for _ in 0..steps {
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
            spent.add(&self.step_cost);
            *frame += 1;
        }
    }

    /// Each end's worst step of `MATCH_TICKS` ticks of the lane 1v1, as the match scenario
    /// plays it: the rollback of each avatar's death falls in the clients'. The server and
    /// the clients keep their data, in a test directory of its own, so the journal and the
    /// receipts write as a real match's do; it is removed after.
    pub(crate) fn worst_1v1() -> StepCost {
        let test_dir = TestDir::new();
        let mut local = Self::new(InProcessSetup::duo(LinkModel::perfect(), Self::SEED_CHAIN));
        local.keep_data(test_dir.path("data"));
        local.start_match();
        local.play_by_team(Self::SCENARIO_SCRIPTS);
        let mut worst = StepCost::default();
        for _ in 0..MATCH_TICKS {
            local.step();
            worst.keep_worst(&local.step_cost);
        }
        drop(local);
        worst
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_cost_adds_and_keeps_the_worst_of_each_end() {
        let micros = Duration::from_micros;
        let step = |clients: &[u64], server: u64| StepCost {
            clients: clients.iter().map(|&each| micros(each)).collect(),
            server: micros(server),
        };
        let steps = [step(&[3, 9], 5), step(&[7, 2], 4)];
        let (mut total, mut worst) = (StepCost::default(), StepCost::default());
        for each in &steps {
            total.add(each);
            worst.keep_worst(each);
        }
        // Each end on its own: client 0 took 3 and 7, client 1 9 and 2, the server 5 and 4.
        assert_eq!(total.clients, [micros(10), micros(11)]);
        assert_eq!(total.server, micros(9));
        assert_eq!(worst.clients, [micros(7), micros(9)]);
        assert_eq!(worst.server, micros(5));
        assert_eq!(worst.worst_client(), micros(9));
        total.clear();
        assert_eq!((total.clients.len(), total.server), (0, Duration::ZERO));
    }
}
