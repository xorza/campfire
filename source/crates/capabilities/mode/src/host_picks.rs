use crate::mode_data::ModeData;
use crate::option_picks::OptionPicks;
use crate::seat_picks::SeatPicks;

/// What a session's host picked of its mode: a value of each option, and a value of each choice
/// of listed values for each seat (08-sessions.md, The host picks, and the terms record the pick;
/// 08-sessions.md, The terms hold each seat's picks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPicks {
    pub options: OptionPicks,
    pub seats: SeatPicks,
}

impl HostPicks {
    /// The default of each of the options of `data`, and no seat's pick of its choices, for
    /// `seats` seats.
    pub fn defaults(data: &ModeData, seats: usize) -> Self {
        Self {
            options: OptionPicks::defaults(&data.options),
            seats: SeatPicks::none(&data.choices, seats),
        }
    }
}
