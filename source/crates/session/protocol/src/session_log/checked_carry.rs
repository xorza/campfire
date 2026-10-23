use campfire_common::{PlayerSlot, Ticks};

use crate::checkpoint::Checkpoint;
use crate::checkpoint::log_carry::{CarriedInput, CarriedPlayerSlot, LogCarry};
use crate::control::Control;
use crate::main_key::MainKey;
use crate::session_id::SessionId;
use crate::session_log::error::CarryFlaw;
use crate::session_terms::SessionTerms;
use crate::stamp_count::StampCount;

/// A checkpoint's carry that holds a state a log holds at the checkpoint's boundary, the one shape
/// a log takes a carry in: each player slot's chain is its own and counts its player's stamps, each
/// count is within what the boundary's ticks hold, each player's main key controls one player slot,
/// and each input still to apply is of a player slot, due from the boundary on, in the order it
/// applies, and within the session's input rules.
#[derive(Debug)]
pub(super) struct CheckedCarry(LogCarry);

/// What a carry is checked against: the log's session and the count of its player slots.
#[derive(Debug, Clone, Copy)]
pub(super) struct CarryCx<'a> {
    pub(super) terms: &'a SessionTerms,
    pub(super) session_id: &'a SessionId,
    pub(super) player_slots: usize,
}

impl CheckedCarry {
    /// The carry of `record`, once it holds a state a log holds at the record's boundary.
    pub(super) fn new(record: &Checkpoint, cx: CarryCx<'_>) -> Result<Self, CarryFlaw> {
        let carry = &record.carry;
        if carry.player_slots.len() != cx.player_slots {
            return Err(CarryFlaw::PlayerSlots);
        }
        let rules = cx.terms.inputs;
        // The most inputs a player sends by the boundary: its ticks to the max lead past it fill,
        // as a packet's `AheadOfTime` bounds them.
        let most = record
            .tick
            .after(rules.max_lead)
            .after(Ticks::new(1))
            .get()
            .saturating_mul(u64::from(rules.max_per_tick));
        for (player_slot, carried) in (0..).map(PlayerSlot::new).zip(&carry.player_slots) {
            Self::check_player_slot(player_slot, carried, &cx, most)?;
            let held_twice = carry.player_slots[..player_slot.index()]
                .iter()
                .filter_map(|other| main_key(other))
                .any(|other| main_key(carried) == Some(other));
            if held_twice {
                return Err(CarryFlaw::MainKeyHeld(player_slot));
            }
        }
        let mut run = 0;
        for (at, input) in carry.pending.iter().enumerate() {
            let before = at.checked_sub(1).map(|before| &carry.pending[before]);
            let same = before.is_some_and(|before| {
                (before.tick, before.player_slot) == (input.tick, input.player_slot)
            });
            run = if same { run + 1 } else { 1 };
            if run > rules.max_per_tick || !Self::holds(input, before, record, &cx) {
                return Err(CarryFlaw::Pending(at));
            }
        }
        Ok(Self(carry.clone()))
    }

    fn check_player_slot(
        player_slot: PlayerSlot,
        carried: &CarriedPlayerSlot,
        cx: &CarryCx<'_>,
        most: u64,
    ) -> Result<(), CarryFlaw> {
        let rules = cx.terms.inputs;
        let stamps = carried.stamps;
        if carried.spill.count > rules.max_per_tick {
            return Err(CarryFlaw::Spill(player_slot));
        }
        match &carried.control {
            Control::Player { delegation, chain } => {
                delegation
                    .check(&cx.terms.server_key, cx.session_id)
                    .map_err(|_| CarryFlaw::Scope(player_slot))?;
                if chain.player_slot() != player_slot || chain.next_seq() != stamps.total {
                    return Err(CarryFlaw::Chain(player_slot));
                }
                let counted = match stamps.last {
                    Some(_) => (1..=rules.max_per_tick).contains(&stamps.at_last),
                    None => stamps.at_last == 0 && stamps.total == 0,
                };
                let past = stamps.total > most || stamps.total == u64::MAX;
                if !counted || u64::from(stamps.at_last) > stamps.total || past {
                    return Err(CarryFlaw::Stamps(player_slot));
                }
            }
            Control::Bot | Control::Open | Control::Reserved => {
                if stamps != StampCount::default() {
                    return Err(CarryFlaw::Stamps(player_slot));
                }
            }
        }
        Ok(())
    }

    /// Whether `input`, after `before`, is one a log holds still to apply at `record`'s boundary.
    fn holds(
        input: &CarriedInput,
        before: Option<&CarriedInput>,
        record: &Checkpoint,
        cx: &CarryCx<'_>,
    ) -> bool {
        let rules = cx.terms.inputs;
        let applies = record
            .carry
            .player_slots
            .get(input.player_slot.index())
            .is_some_and(|player_slot| {
                matches!(player_slot.control, Control::Player { .. } | Control::Bot)
            });
        let in_order = before.is_none_or(|before| {
            (before.tick, before.player_slot) <= (input.tick, input.player_slot)
        });
        let fits = u64::try_from(input.payload.len())
            .is_ok_and(|len| len <= u64::from(rules.max_payload_len));
        applies && input.tick >= record.tick && in_order && fits
    }

    pub(super) fn into_inner(self) -> LogCarry {
        self.0
    }
}

/// The main key of the player who controls `player_slot`, if one does.
fn main_key(player_slot: &CarriedPlayerSlot) -> Option<&MainKey> {
    match &player_slot.control {
        Control::Player { delegation, .. } => Some(delegation.main_key()),
        Control::Bot | Control::Open | Control::Reserved => None,
    }
}
