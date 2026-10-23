use std::collections::BTreeMap;

use campfire_common::{DeclaredName, PlayerSlot};

use crate::choice_data::{ChoiceData, Offers};
use crate::error::PlayerSlotPickError;
use crate::player_slot_pick::PlayerSlotPick;

/// The value a session's host picked of each choice of listed values for each player slot, by its
/// place among the choice's values, `NONE` where it picked none: player slot by player slot, each
/// player slot's choices of values in the order of their names (08-sessions.md, The terms hold each
/// player slot's picks). Only its constructors make one, so each place is one of its choice's, and
/// no two player slots hold one value of a unique choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerSlotPicks(Box<[u8]>);

/// A player slot's pick: the player slot, the place of its choice among the choices of values, in
/// the order of their names, and the place of its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Picked {
    pub(crate) player_slot: PlayerSlot,
    pub(crate) listed: usize,
    pub(crate) place: u8,
}

impl PlayerSlotPicks {
    /// The place of no pick.
    pub const NONE: u8 = u8::MAX;

    /// No pick of `choices` for any of `player_slots` player slots.
    pub fn none(choices: &BTreeMap<DeclaredName, ChoiceData>, player_slots: usize) -> Self {
        Self(vec![Self::NONE; Self::listed(choices).count() * player_slots].into())
    }

    /// The picks of `values`, each of a player slot of `player_slots` by its value, of `choices`,
    /// and no pick for the rest; an error for a player slot past `player_slots`, a choice that
    /// lists no values, a value its choice lacks, one player slot's choice picked twice, or two
    /// player slots' one value of a unique choice.
    pub fn named(
        choices: &BTreeMap<DeclaredName, ChoiceData>,
        player_slots: usize,
        values: &[PlayerSlotPick],
    ) -> Result<Self, PlayerSlotPickError> {
        let mut picks = Self::none(choices, player_slots);
        let width = Self::listed(choices).count();
        for PlayerSlotPick {
            player_slot,
            choice,
            value,
        } in values
        {
            if player_slot.index() >= player_slots {
                return Err(PlayerSlotPickError::PlayerSlot(*player_slot));
            }
            let (at, data) = Self::listed(choices)
                .enumerate()
                .find(|(_, (name, _))| *name == choice)
                .map(|(at, (_, data))| (at, data))
                .ok_or_else(|| PlayerSlotPickError::Unknown(choice.clone()))?;
            let place = data
                .values
                .iter()
                .position(|held| held == value)
                .ok_or_else(|| PlayerSlotPickError::NoValue {
                    choice: choice.clone(),
                    value: value.clone(),
                })?;
            let held = &mut picks.0[player_slot.index() * width + at];
            if *held != Self::NONE {
                return Err(PlayerSlotPickError::Twice {
                    player_slot: *player_slot,
                    choice: choice.clone(),
                });
            }
            *held = u8::try_from(place).expect("a choice lists fewer values than a byte's");
        }
        Self::checked(choices, player_slots, &picks.0)
    }

    /// `places`, as a session's terms carry them, the picks of `choices` for `player_slots` player
    /// slots; an error for another count than the player slots' choices of values, a place past its
    /// choice's values, or two player slots' one value of a unique choice.
    pub fn checked(
        choices: &BTreeMap<DeclaredName, ChoiceData>,
        player_slots: usize,
        places: &[u8],
    ) -> Result<Self, PlayerSlotPickError> {
        let width = Self::listed(choices).count();
        let expected = width * player_slots;
        if places.len() != expected {
            return Err(PlayerSlotPickError::Count {
                given: places.len(),
                expected,
            });
        }
        for (at, (name, data)) in Self::listed(choices).enumerate() {
            let column = places.iter().skip(at).step_by(width);
            let mut picked = Vec::with_capacity(player_slots);
            for (player_slot, &place) in (0..).map(PlayerSlot::new).zip(column) {
                if place == Self::NONE {
                    continue;
                }
                if usize::from(place) >= data.values.len() {
                    let choice = name.clone();
                    return Err(PlayerSlotPickError::Place {
                        player_slot,
                        choice,
                        place,
                    });
                }
                if data.unique && picked.contains(&place) {
                    let choice = name.clone();
                    return Err(PlayerSlotPickError::Taken { choice, place });
                }
                picked.push(place);
            }
        }
        Ok(Self(places.into()))
    }

    /// The places, as a session's terms carry them.
    pub fn places(&self) -> &[u8] {
        &self.0
    }

    /// Each pick of `choices`, the ones it was made of, player slot by player slot, each player
    /// slot's in the order of the choices' names.
    pub(crate) fn picked(
        &self,
        choices: &BTreeMap<DeclaredName, ChoiceData>,
    ) -> impl Iterator<Item = Picked> {
        let width = Self::listed(choices).count();
        let picked = self.0.iter().enumerate();
        picked
            .filter(|&(_, &place)| place != Self::NONE)
            .map(move |(at, &place)| Picked {
                player_slot: PlayerSlot::new(
                    u32::try_from(at / width).expect("a player slot fits u32"),
                ),
                listed: at % width,
                place,
            })
    }

    /// The choices of listed values, in the order of their names.
    fn listed(
        choices: &BTreeMap<DeclaredName, ChoiceData>,
    ) -> impl Iterator<Item = (&DeclaredName, &ChoiceData)> {
        choices
            .iter()
            .filter(|(_, choice)| choice.offers == Offers::Values)
    }
}

#[cfg(test)]
mod tests;
