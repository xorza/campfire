//! The allocation audit of a match of the rules alone, as `Match::builder` builds one: its units
//! walk, fight, die and respawn, and no tick past the warmup allocates but what Bevy allocates
//! for it (02-engine-core.md, Allocation audits). Rhai's own allocations stay with the script
//! benches. The allocator counts every thread of the process but its main one, as nextest runs
//! each test in a process of its own.

use campfire_capabilities::{Match, Order};
use campfire_combat::internals::Armed;
use campfire_combat::{Combat, OnDeath, Respawn};
use campfire_common::{PlayerSlot, Ticks};
use campfire_math::{Num, Vec3};
use campfire_measure::{AllocAudit, Allowed, CallSite, CountingAllocator};
use campfire_orders::{Orders, OrdersDirective};
use campfire_players::OrderUnits;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, SimWorld, StableId};
use campfire_units::internals::Spawn;
use campfire_units::{Dead, GroundPoint, MoveStep, Owner, SpawnPoint, Units};
use campfire_values::Team;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

/// The units on each side.
const SIDE: i64 = 8;
/// The ticks a dead unit waits before it respawns.
const RESPAWN: u64 = 20;
/// The ticks between two orders to each side to attack-move across: a respawned unit stands
/// until the next.
const ORDER_PERIOD: u64 = 40;
/// The ticks past the last growth of the match's buffers: two sides 24 m apart, at half a meter
/// a tick, meet in 24 ticks; a unit of 100 life takes 25 each 5 ticks, so a fight ends in about
/// 20, and units come back 20 later and walk out again within 40.
const WARMUP: usize = 600;
/// The ticks the audit measures, several rounds of fights.
const FRAMES: usize = 256;

/// What a tick allocates: Bevy's `BundleRemover::remove` collects into a `Vec` the components a
/// removal takes from an archetype that a `Remove` observer watches, as each unit's archetype is,
/// and no two units of the battle respawn in one tick. bevyengine/bevy#26301 removes it; the
/// site names the version, so a new Bevy checks the budget again.
const BEVY: [Allowed; 1] = [Allowed {
    site: CallSite::File("bevy_ecs-0.20.0/src/bundle/remove.rs"),
    why: "the components a respawn's removal of `Dead` and `Respawn` takes",
}];

/// Two sides of `SIDE` fighters each, 24 m apart, each a player's, and the order of each side to
/// attack-move to the far side.
#[derive(Debug)]
struct Battle {
    game: Match,
    units: Vec<StableId>,
    orders: [(PlayerSlot, Vec<u8>); 2],
}

impl Battle {
    fn new() -> Self {
        let declared = [
            Capability::Stats,
            Capability::Combat,
            Capability::Navigation,
            Capability::Orders,
        ];
        let mut game = Match::builder(&declared).players(2).build();
        Combat::load_weapon_stats(&mut game);
        let fighter = Armed::melee(Num::int(100), Num::int(2), 1, 5, Num::int(25))
            .on_death(OnDeath::Stay)
            .load(&mut game);
        Units::give_type(&mut game, fighter, MoveStep::new(Num::HALF).unwrap());
        Orders::load_acquire(&mut game, fighter, 100, &["enemies"]);
        let mut units = Vec::new();
        let orders = [(0, -12, 16), (1, 12, -16)].map(|(side, from, to)| {
            let slot = PlayerSlot::new(side);
            let side_units: Vec<StableId> = (0..SIDE)
                .map(|nth| {
                    let at = Vec3::new(Num::int(from), Num::ZERO, Num::int(2 * nth));
                    Spawn::of(fighter, at)
                        .team(Team::new(u8::try_from(side).unwrap()))
                        .parts((Owner::new(slot), SpawnPoint::new(at)))
                        .place(&mut game)
                })
                .collect();
            units.extend(&side_units);
            let to = GroundPoint {
                x: Num::int(to),
                z: Num::int(SIDE - 1),
            };
            let order = Order {
                units: OrderUnits::new(&side_units).unwrap(),
                directive: OrdersDirective::AttackMove { to }.into(),
            };
            (slot, Order::payload(&[order]))
        });
        Self {
            game,
            units,
            orders,
        }
    }

    /// One tick: each side's order on its period, and each unit dead with no respawn set comes
    /// back `RESPAWN` ticks later. Whether a unit came back.
    fn tick(&mut self) -> bool {
        let now = self.game.now();
        let mut respawned = false;
        for &id in &self.units {
            if self.game.try_part::<Dead>(id).is_some()
                && self.game.try_part::<Respawn>(id).is_none()
            {
                let at = now.after(Ticks::new(RESPAWN));
                self.game.insert_parts(id, Respawn { at });
                respawned = true;
            }
        }
        if now.get().is_multiple_of(ORDER_PERIOD) {
            let [(first, first_order), (second, second_order)] = &self.orders;
            self.game
                .step_with(&[(*first, first_order), (*second, second_order)]);
        } else {
            self.game.step();
        }
        respawned
    }
}

#[test]
fn a_battle_of_the_rules_allocates_nothing_a_tick() {
    let mut battle = Battle::new();
    let mut ticks = 0;
    let mut last_respawn = 0;
    AllocAudit::new(WARMUP)
        .frames(FRAMES)
        .budget(&BEVY)
        .run(|| {
            if battle.tick() {
                last_respawn = ticks;
            }
            ticks += 1;
        });
    // The fight goes on through the measured ticks: a unit died and came back among them.
    assert!(last_respawn >= WARMUP, "{last_respawn}");
}
