//! Snapshots of the proving match with flawed values: each restores or is refused, and what
//! restores plays on with no panic. Each sweep runs its cases on every core, each thread on a
//! match of its own (02-engine-core.md, A sweep runs on every core).

use std::collections::BTreeMap;

use bevy_ecs::world::World;
use campfire_actions::{ActionSlots, ActionTarget};
use campfire_capabilities::{Match, Order};
use campfire_common::PlayerSlot;
use campfire_harness::{FixedMatch, MatchUnits, ProvingMatch, ScriptedMode};
use campfire_orders::OrdersDirective;
use campfire_runner::MatchBuild;
use campfire_sim::internals::{Draws, Mutation, SimWorldInternals};
use campfire_sim::{Position, SimWorld};
use campfire_threads::internals::Parallel;
use campfire_units::{Dead, Owner};

/// The draws of each state type: enough to reach each edge of its numbers, few enough to keep
/// the test fast.
const DRAWS: usize = 16;

/// The ticks a restored match plays, its first with an order of every slot of every unit.
const WINDOW: usize = 5;

/// The draws of each mutation of each state type: each picks its holder and its other entity anew.
const MUTATIONS: usize = 4;

/// What a case of a sweep came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Case {
    /// The case made no flawed snapshot: its state type has nothing to flaw that way.
    Skipped,
    Restored,
    Refused,
}

/// The counts of a sweep's cases that made a flawed snapshot.
#[derive(Debug, Default)]
struct Cases {
    restored: usize,
    refused: usize,
}

impl Cases {
    fn of(cases: impl IntoIterator<Item = Case>) -> Self {
        let mut counts = Self::default();
        for case in cases {
            match case {
                Case::Skipped => {}
                Case::Restored => counts.restored += 1,
                Case::Refused => counts.refused += 1,
            }
        }
        counts
    }

    /// Both outcomes are reached, so the checks are exercised, not passed over.
    fn assert_both(&self) {
        assert!(
            self.restored > 0 && self.refused > 0,
            "{} restored, {} refused",
            self.restored,
            self.refused
        );
    }
}

/// The proving match past the first trains, with heroes, guards, creeps, projectiles and
/// modifiers about, and its snapshot, which each case flaws.
#[derive(Debug)]
struct Base {
    proving: ScriptedMode<ProvingMatch>,
    fixed: FixedMatch,
    snapshot: Vec<u8>,
}

impl Base {
    fn new() -> Self {
        let proving = ProvingMatch::load();
        let mut fixed = proving.start();
        for _ in 0..60 {
            proving.play_tick(&mut fixed);
        }
        let mut snapshot = Vec::new();
        fixed.game().snapshot(&mut snapshot);
        Self {
            proving,
            fixed,
            snapshot,
        }
    }

    /// A match built from the proving match's packages, which a case restores its snapshot in.
    fn target(&self) -> Match {
        MatchBuild::test_target(self.proving.packages(), ProvingMatch::PLAYERS)
    }
}

/// The draws of the case at `index`: each case draws from a seed of its own, so it repeats
/// whichever thread runs it.
fn draws(seed: u64, index: usize) -> Draws {
    Draws::new(seed + u64::try_from(index).unwrap())
}

/// A snapshot of the proving match restores into a match built from its packages, to the same
/// hash; and a snapshot with any byte flipped never panics as it restores or as it plays on, and
/// what restores snapshots again to the same bytes: no two byte strings restore to one state.
#[test]
fn a_snapshot_restores_and_a_flawed_one_never_panics() {
    let base = Base::new();
    let bytes = &base.snapshot;
    let mut target = base.target();
    target.restore_in_place(bytes).unwrap();
    assert_eq!(target.state_hash(), base.fixed.game().state_hash());

    // Every byte, its low bit flipped, which keeps the most values decodable, so the most restore
    // and play on: a flip of the high bit or of every bit restores in few of the types the low
    // bit's reach, and is refused in the same ways. The drawn values below reach each type's edges.
    Parallel::map_init(
        bytes.len(),
        || (base.target(), Vec::new(), Vec::new()),
        |(target, flawed, again), at| {
            flawed.clear();
            flawed.extend_from_slice(bytes);
            flawed[at] ^= 0x01;
            if target.restore_in_place(flawed).is_ok() {
                again.clear();
                target.snapshot(again);
                assert_eq!(again, flawed, "byte {at} flipped");
                // A value its checks let through still plays: no system panics on it.
                for _ in 0..WINDOW {
                    target.step();
                }
            }
        },
    );
}

/// Drawn values of each state type in turn, each a value its decode accepts with every number at
/// an edge such as the limit, put in place of one holder's in a snapshot of the proving match, or
/// on a drawn entity when none holds one: each restores or is refused, and what restores plays on
/// with no panic, its first tick with every unit's every slot ordered, so its actions and their
/// effects compute on the drawn value. A byte flip seldom makes a value that decodes, so this
/// reaches the checks the flips do not. Each draw takes a seed of its own, so a case repeats
/// whichever thread runs it.
#[test]
fn a_snapshot_of_drawn_values_restores_or_is_refused_and_never_panics() {
    let base = Base::new();
    let names = base.target().state_names();
    // Each draw of each state type is a job of its own, so every core takes a share however few
    // the types are.
    let cases = Parallel::map_init(
        names.len() * DRAWS,
        || (base.target(), Vec::new()),
        |(target, flawed), job| {
            let mut draws = draws(7, job);
            target.restore_in_place(&base.snapshot).unwrap();
            if !target.scramble_state(names[job / DRAWS], &mut draws) {
                return Case::Skipped;
            }
            flawed.clear();
            target.snapshot(flawed);
            if target.restore_in_place(flawed).is_err() {
                return Case::Refused;
            }
            let orders = every_slot_orders(target);
            let inputs: Vec<(PlayerSlot, &[u8])> = orders
                .iter()
                .map(|(slot, payload)| (*slot, payload.as_slice()))
                .collect();
            target.step_with(&inputs);
            for _ in 1..WINDOW {
                target.step();
            }
            Case::Restored
        },
    );
    // Every state type is drawn, those the match holds nowhere among them.
    let undrawn: Vec<&str> = (names.iter().zip(cases.chunks(DRAWS)))
        .filter(|(_, cases)| cases.iter().all(|&case| case == Case::Skipped))
        .map(|(&name, _)| name)
        .collect();
    assert_eq!(undrawn, Vec::<&str>::new());
    Cases::of(cases).assert_both();
}

/// For each player, an order of each slot of each living unit it controls, at a living unit of
/// the match: a slot's at the unit as many places past it, by stable id, as the slot's number,
/// plus one, so the slots of a unit meet different units.
fn every_slot_orders(world: &World) -> Vec<(PlayerSlot, Vec<u8>)> {
    let units: Vec<_> = MatchUnits::of(world)
        .all()
        .filter(|(_, unit)| unit.contains::<Position>() && !unit.contains::<Dead>())
        .collect();
    let mut by_player: BTreeMap<PlayerSlot, Vec<Order>> = BTreeMap::new();
    for (at, (id, unit)) in units.iter().enumerate() {
        let (Some(owner), Some(slots)) = (unit.get::<Owner>(), unit.get::<ActionSlots>()) else {
            continue;
        };
        for slot in 0..slots.iter().count() {
            let (target, _) = units[(at + slot + 1) % units.len()];
            let slot = u8::try_from(slot).expect("a unit's slots fit u8");
            let action = OrdersDirective::Slot {
                slot,
                target: ActionTarget::Unit(target),
            };
            by_player
                .entry(owner.player_slot())
                .or_default()
                .push(Order::one(*id, action));
        }
    }
    by_player
        .into_iter()
        .map(|(slot, orders)| (slot, Order::payload(&orders)))
        .collect()
}

/// Each state type's value removed from one holder, moved to an entity that holds none, copied to
/// one, or with one of its `Some`s as `None`, in a snapshot of the proving match: each restores
/// or is refused, and what restores plays on with no panic. A value draw keeps each value where
/// it sat and draws every part anew, so this reaches the checks of where a value may sit, which
/// parts a holder needs, and an option a system expects to hold. Each state type draws from a
/// seed of its own.
#[test]
fn a_snapshot_with_a_value_moved_or_an_option_dropped_restores_or_is_refused_and_never_panics() {
    let base = Base::new();
    let names = base.target().state_names();
    let cases = Parallel::map_init(
        names.len(),
        || (base.target(), Vec::new()),
        |(target, flawed), index| {
            let mut draws = draws(11, index);
            let mut made = [0; Mutation::ALL.len()];
            let mut cases = Vec::new();
            for (kind, mutation) in Mutation::ALL.into_iter().enumerate() {
                for _ in 0..MUTATIONS {
                    target.restore_in_place(&base.snapshot).unwrap();
                    if !target.mutate_state(names[index], &mut draws, mutation) {
                        continue;
                    }
                    made[kind] += 1;
                    flawed.clear();
                    target.snapshot(flawed);
                    if target.restore_in_place(flawed).is_ok() {
                        cases.push(Case::Restored);
                        for _ in 0..WINDOW {
                            target.step();
                        }
                    } else {
                        cases.push(Case::Refused);
                    }
                }
            }
            (made, cases)
        },
    );
    let mut made = [0; Mutation::ALL.len()];
    for (case, _) in &cases {
        for (total, count) in made.iter_mut().zip(case) {
            *total += count;
        }
    }
    // Each mutation reaches some state type of the match.
    assert!(made.iter().all(|&count| count > 0), "{made:?}");
    Cases::of(cases.into_iter().flat_map(|(_, cases)| cases)).assert_both();
}
