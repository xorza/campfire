use campfire_common::{AfterLeave, PlayerSlot, Tick};

use crate::control::{Control, Controller};
use crate::input_chain::InputChain;
use crate::main_key::MainKey;
use crate::session_log::log_entries::LogEntries;
use crate::stamp_count::StampCount;

/// Who controls each player slot of a session, as its log follows it from the header and the
/// server's inputs, by player slot: a player, a bot, or none, with the counts of its player's
/// inputs.
#[derive(Debug, Default)]
pub(super) struct PlayerSlotControl {
    player_slots: Vec<PlayerSlotState>,
}

/// A player slot as the log follows it: who controls it, the main key of the player who left it
/// last, and the stamps of its player's inputs.
#[derive(Debug, Clone, Copy)]
pub(super) struct PlayerSlotState {
    /// Who controls it, a player by the index of their current delegation among the log's.
    pub(super) control: Control<u32>,
    pub(super) leaver: Option<MainKey>,
    pub(super) stamps: StampCount,
    /// The delegation whose key signed the head of its player's chain, by index; none before
    /// their first input.
    pub(super) head_signer: Option<u32>,
}

/// What a player slot holds once it takes its player's packet: the delegation whose key signed it,
/// the player's chain after it, and the stamps of their inputs.
#[derive(Debug, Clone, Copy)]
pub(super) struct TakenPacket {
    pub(super) delegation: u32,
    pub(super) chain: InputChain,
    pub(super) stamps: StampCount,
}

impl PlayerSlotState {
    /// A player slot of `control` with no input yet.
    pub(super) fn of(control: Control<u32>) -> Self {
        Self {
            control,
            leaver: None,
            stamps: StampCount::default(),
            head_signer: None,
        }
    }
}

impl FromIterator<PlayerSlotState> for PlayerSlotControl {
    fn from_iter<I: IntoIterator<Item = PlayerSlotState>>(player_slots: I) -> Self {
        Self {
            player_slots: player_slots.into_iter().collect(),
        }
    }
}

impl PlayerSlotControl {
    /// The player slot `player_slot`; none for a player slot the session does not have.
    pub(super) fn get(&self, player_slot: PlayerSlot) -> Option<&PlayerSlotState> {
        self.player_slots.get(player_slot.index())
    }

    /// Each player slot, in order.
    pub(super) fn iter(&self) -> impl ExactSizeIterator<Item = &PlayerSlotState> {
        self.player_slots.iter()
    }

    /// How many player slots the session has.
    pub(super) const fn len(&self) -> usize {
        self.player_slots.len()
    }

    /// Who controls `player_slot` now, its delegations in `entries`; none for a player slot the
    /// session does not have.
    pub(super) fn controller<'e>(
        &self,
        player_slot: PlayerSlot,
        entries: &'e LogEntries,
    ) -> Option<Controller<'e>> {
        Some(
            self.get(player_slot)?
                .control
                .map(|delegation| entries.delegation(delegation)),
        )
    }

    /// The main key of the player who left `player_slot` last, while no player took it since.
    pub(super) fn leaver(&self, player_slot: PlayerSlot) -> Option<MainKey> {
        self.get(player_slot)?.leaver
    }

    /// The last stamp of the inputs `player_slot`'s player sent since it joined.
    pub(super) fn last_stamp(&self, player_slot: PlayerSlot) -> Option<Tick> {
        self.get(player_slot)?.stamps.last
    }

    /// The player slots a player controls now, in order.
    pub(super) fn played_player_slots(&self) -> impl Iterator<Item = PlayerSlot> {
        (0..)
            .zip(&self.player_slots)
            .filter(|(_, player_slot)| matches!(player_slot.control, Control::Player { .. }))
            .map(|(player_slot, _)| PlayerSlot::new(player_slot))
    }

    /// Whether a player of `main_key`, its delegations in `entries`, controls a player slot.
    pub(super) fn holds(&self, main_key: &MainKey, entries: &LogEntries) -> bool {
        self.player_slots
            .iter()
            .any(|player_slot| match player_slot.control {
                Control::Player { delegation, .. } => {
                    entries.delegation(delegation).main_key() == main_key
                }
                Control::Bot | Control::Open | Control::Reserved => false,
            })
    }

    /// Takes `player_slot`'s packet, as `taken` holds it.
    pub(super) fn took_packet(&mut self, player_slot: PlayerSlot, taken: TakenPacket) {
        let TakenPacket {
            delegation,
            chain,
            stamps,
        } = taken;
        let held = &mut self.player_slots[player_slot.index()];
        held.control = Control::Player { delegation, chain };
        held.stamps = stamps;
        held.head_signer = Some(delegation);
    }

    /// A player of the delegation `delegation` joins `player_slot`, in the chain `chain`.
    pub(super) fn join(&mut self, player_slot: PlayerSlot, delegation: u32, chain: InputChain) {
        self.player_slots[player_slot.index()] =
            PlayerSlotState::of(Control::Player { delegation, chain });
    }

    /// The player of `player_slot` renews their delegation to `delegation`.
    pub(super) fn renew(&mut self, player_slot: PlayerSlot, delegation: u32) {
        if let Control::Player {
            delegation: held, ..
        } = &mut self.player_slots[player_slot.index()].control
        {
            *held = delegation;
        }
    }

    /// The player of `player_slot`, its delegations in `entries`, leaves it, which becomes
    /// `becomes`.
    pub(super) fn leave(
        &mut self,
        player_slot: PlayerSlot,
        becomes: AfterLeave,
        entries: &LogEntries,
    ) {
        let held = &mut self.player_slots[player_slot.index()];
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
