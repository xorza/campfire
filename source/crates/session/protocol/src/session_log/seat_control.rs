use campfire_common::{AfterLeave, Seat, Tick};

use crate::control::{Control, Controller};
use crate::input_chain::InputChain;
use crate::main_key::MainKey;
use crate::session_log::log_entries::LogEntries;
use crate::stamp_count::StampCount;

/// Who controls each seat of a session, as its log follows it from the header and the server's
/// inputs, by seat: a player, a bot, or none, with the counts of its player's inputs.
#[derive(Debug, Default)]
pub(super) struct SeatControl {
    seats: Vec<SeatState>,
}

/// A seat as the log follows it: who controls it, the main key of the player who left it last,
/// and the stamps of its player's inputs.
#[derive(Debug, Clone, Copy)]
pub(super) struct SeatState {
    /// Who controls it, a player by the index of their current delegation among the log's.
    pub(super) control: Control<u32>,
    pub(super) leaver: Option<MainKey>,
    pub(super) stamps: StampCount,
    /// The delegation whose key signed the head of its player's chain, by index; none before
    /// their first input.
    pub(super) head_signer: Option<u32>,
}

/// What a seat holds once it takes its player's packet: the delegation whose key signed it, the
/// player's chain after it, and the stamps of their inputs.
#[derive(Debug, Clone, Copy)]
pub(super) struct TakenPacket {
    pub(super) delegation: u32,
    pub(super) chain: InputChain,
    pub(super) stamps: StampCount,
}

impl SeatState {
    /// A seat of `control` with no input yet.
    pub(super) fn of(control: Control<u32>) -> Self {
        Self {
            control,
            leaver: None,
            stamps: StampCount::default(),
            head_signer: None,
        }
    }
}

impl FromIterator<SeatState> for SeatControl {
    fn from_iter<I: IntoIterator<Item = SeatState>>(seats: I) -> Self {
        Self {
            seats: seats.into_iter().collect(),
        }
    }
}

impl SeatControl {
    /// The seat `seat`; none for a seat the session does not have.
    pub(super) fn get(&self, seat: Seat) -> Option<&SeatState> {
        self.seats.get(seat.index())
    }

    /// Each seat, in order.
    pub(super) fn iter(&self) -> impl ExactSizeIterator<Item = &SeatState> {
        self.seats.iter()
    }

    /// How many seats the session has.
    pub(super) const fn len(&self) -> usize {
        self.seats.len()
    }

    /// Who controls `seat` now, its delegations in `entries`; none for a seat the session does
    /// not have.
    pub(super) fn controller<'e>(
        &self,
        seat: Seat,
        entries: &'e LogEntries,
    ) -> Option<Controller<'e>> {
        Some(
            self.get(seat)?
                .control
                .map(|delegation| entries.delegation(delegation)),
        )
    }

    /// The main key of the player who left `seat` last, while no player took it since.
    pub(super) fn leaver(&self, seat: Seat) -> Option<MainKey> {
        self.get(seat)?.leaver
    }

    /// The last stamp of the inputs `seat`'s player sent since it joined.
    pub(super) fn last_stamp(&self, seat: Seat) -> Option<Tick> {
        self.get(seat)?.stamps.last
    }

    /// The seats a player controls now, in order.
    pub(super) fn player_seats(&self) -> impl Iterator<Item = Seat> {
        (0..)
            .zip(&self.seats)
            .filter(|(_, seat)| matches!(seat.control, Control::Player { .. }))
            .map(|(seat, _)| Seat::new(seat))
    }

    /// Whether a player of `main_key`, its delegations in `entries`, controls a seat.
    pub(super) fn holds(&self, main_key: &MainKey, entries: &LogEntries) -> bool {
        self.seats.iter().any(|seat| match seat.control {
            Control::Player { delegation, .. } => {
                entries.delegation(delegation).main_key() == main_key
            }
            Control::Bot | Control::Open | Control::Reserved => false,
        })
    }

    /// Takes `seat`'s packet, as `taken` holds it.
    pub(super) fn took_packet(&mut self, seat: Seat, taken: TakenPacket) {
        let TakenPacket {
            delegation,
            chain,
            stamps,
        } = taken;
        let held = &mut self.seats[seat.index()];
        held.control = Control::Player { delegation, chain };
        held.stamps = stamps;
        held.head_signer = Some(delegation);
    }

    /// A player of the delegation `delegation` joins `seat`, in the chain `chain`.
    pub(super) fn join(&mut self, seat: Seat, delegation: u32, chain: InputChain) {
        self.seats[seat.index()] = SeatState::of(Control::Player { delegation, chain });
    }

    /// The player of `seat` renews their delegation to `delegation`.
    pub(super) fn renew(&mut self, seat: Seat, delegation: u32) {
        if let Control::Player {
            delegation: held, ..
        } = &mut self.seats[seat.index()].control
        {
            *held = delegation;
        }
    }

    /// The player of `seat`, its delegations in `entries`, leaves it, which becomes `becomes`.
    pub(super) fn leave(&mut self, seat: Seat, becomes: AfterLeave, entries: &LogEntries) {
        let held = &mut self.seats[seat.index()];
        if let Control::Player { delegation, .. } = held.control {
            held.leaver = Some(*entries.delegation(delegation).main_key());
        }
        held.control = match becomes {
            AfterLeave::Reserve => Control::Reserved,
            AfterLeave::Bot => Control::Bot,
            AfterLeave::Open => Control::Open,
        };
        held.stamps = StampCount::default();
    }
}
