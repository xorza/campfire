use campfire_common::{AfterLeave, PlayerSlot, Tick};

use crate::controller::Controller;
use crate::input_chain::InputChain;
use crate::main_key::MainKey;
use crate::session_log::log_entries::LogEntries;
use crate::stamp_count::StampCount;

/// Who controls each slot of a session, as its log follows it from the header and the server's
/// inputs, by slot: a player, a bot, or none, with the counts of its player's inputs.
#[derive(Debug, Default)]
pub(super) struct SlotControl {
    slots: Vec<Slot>,
}

/// A slot as the log follows it: who controls it, the main key of the player who left it last,
/// and the stamps of its player's inputs.
#[derive(Debug, Clone, Copy)]
pub(super) struct Slot {
    pub(super) control: Control,
    pub(super) leaver: Option<MainKey>,
    pub(super) stamps: StampCount,
    /// The delegation whose key signed the head of its player's chain, by index; none before
    /// their first input.
    pub(super) head_signer: Option<u32>,
}

/// Who controls a slot.
#[derive(Debug, Clone, Copy)]
pub(super) enum Control {
    /// A player, by the index of their current delegation, with their chain as logged so far.
    Player {
        delegation: u32,
        chain: InputChain,
    },
    Bot,
    Open,
    /// None: it waits for the player who left it.
    Reserved,
}

/// What a slot holds once it takes its player's packet: the delegation whose key signed it, the
/// player's chain after it, and the stamps of their inputs.
#[derive(Debug, Clone, Copy)]
pub(super) struct TakenPacket {
    pub(super) delegation: u32,
    pub(super) chain: InputChain,
    pub(super) stamps: StampCount,
}

impl Slot {
    /// A slot of `control` with no input yet.
    pub(super) fn of(control: Control) -> Self {
        Self {
            control,
            leaver: None,
            stamps: StampCount::default(),
            head_signer: None,
        }
    }
}

impl FromIterator<Slot> for SlotControl {
    fn from_iter<I: IntoIterator<Item = Slot>>(slots: I) -> Self {
        Self {
            slots: slots.into_iter().collect(),
        }
    }
}

impl SlotControl {
    /// The slot `slot`; none for a slot the session does not have.
    pub(super) fn get(&self, slot: PlayerSlot) -> Option<&Slot> {
        self.slots.get(slot.index())
    }

    /// Each slot, in order.
    pub(super) fn iter(&self) -> impl ExactSizeIterator<Item = &Slot> {
        self.slots.iter()
    }

    /// How many slots the session has.
    pub(super) const fn len(&self) -> usize {
        self.slots.len()
    }

    /// Who controls `slot` now, its delegations in `entries`; none for a slot the session does
    /// not have.
    pub(super) fn controller(&self, slot: PlayerSlot, entries: &LogEntries) -> Option<Controller> {
        Some(match self.get(slot)?.control {
            Control::Player { delegation, chain } => {
                let delegation = entries.delegation(delegation);
                Controller::Player {
                    main_key: *delegation.main_key(),
                    session_key: delegation.terms().session_key,
                    chain,
                }
            }
            Control::Bot => Controller::Bot,
            Control::Open => Controller::Open,
            Control::Reserved => Controller::Reserved,
        })
    }

    /// The main key of the player who left `slot` last, while no player took it since.
    pub(super) fn leaver(&self, slot: PlayerSlot) -> Option<MainKey> {
        self.get(slot)?.leaver
    }

    /// The last stamp of the inputs `slot`'s player sent since it joined.
    pub(super) fn last_stamp(&self, slot: PlayerSlot) -> Option<Tick> {
        self.get(slot)?.stamps.last
    }

    /// The slots a player controls now, in order.
    pub(super) fn player_slots(&self) -> impl Iterator<Item = PlayerSlot> {
        (0..)
            .zip(&self.slots)
            .filter(|(_, slot)| matches!(slot.control, Control::Player { .. }))
            .map(|(slot, _)| PlayerSlot::new(slot))
    }

    /// Whether a player of `main_key`, its delegations in `entries`, controls a slot.
    pub(super) fn holds(&self, main_key: &MainKey, entries: &LogEntries) -> bool {
        self.slots.iter().any(|slot| match slot.control {
            Control::Player { delegation, .. } => {
                entries.delegation(delegation).main_key() == main_key
            }
            Control::Bot | Control::Open | Control::Reserved => false,
        })
    }

    /// Takes `slot`'s packet, as `taken` holds it.
    pub(super) fn took_packet(&mut self, slot: PlayerSlot, taken: TakenPacket) {
        let TakenPacket {
            delegation,
            chain,
            stamps,
        } = taken;
        let held = &mut self.slots[slot.index()];
        held.control = Control::Player { delegation, chain };
        held.stamps = stamps;
        held.head_signer = Some(delegation);
    }

    /// A player of the delegation `delegation` joins `slot`, in the chain `chain`.
    pub(super) fn join(&mut self, slot: PlayerSlot, delegation: u32, chain: InputChain) {
        self.slots[slot.index()] = Slot::of(Control::Player { delegation, chain });
    }

    /// The player of `slot` renews their delegation to `delegation`.
    pub(super) fn renew(&mut self, slot: PlayerSlot, delegation: u32) {
        if let Control::Player {
            delegation: held, ..
        } = &mut self.slots[slot.index()].control
        {
            *held = delegation;
        }
    }

    /// The player of `slot`, its delegations in `entries`, leaves it, which becomes `becomes`.
    pub(super) fn leave(&mut self, slot: PlayerSlot, becomes: AfterLeave, entries: &LogEntries) {
        let held = &mut self.slots[slot.index()];
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
