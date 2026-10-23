use crate::mode_data::ModeData;
use crate::option_picks::OptionPicks;
use crate::player_slot_picks::PlayerSlotPicks;

/// What a session's host picked of its mode: a value of each option, and a value of each choice of
/// listed values for each player slot (08-sessions.md, The host picks, and the terms record the
/// pick; 08-sessions.md, The terms hold each player slot's picks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPicks {
    pub options: OptionPicks,
    pub player_slots: PlayerSlotPicks,
}

impl HostPicks {
    /// The default of each of the options of `data`, and no player slot's pick of its choices, for
    /// `player_slots` player slots.
    pub fn defaults(data: &ModeData, player_slots: usize) -> Self {
        Self {
            options: OptionPicks::defaults(&data.options),
            player_slots: PlayerSlotPicks::none(&data.choices, player_slots),
        }
    }
}
