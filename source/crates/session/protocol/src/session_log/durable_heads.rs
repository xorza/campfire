use std::collections::VecDeque;

use campfire_common::Seat;

use crate::session_log::durable_head::DurableHead;

/// Where each seat's chain stands in the records the journal synced, by seat, and the journaled
/// packets not known durable yet, in the order of their records.
#[derive(Debug)]
pub(crate) struct DurableHeads {
    heads: Vec<Option<DurableHead>>,
    journaled: VecDeque<Journaled>,
}

/// A journaled packet: its record's index, its seat, and where it left its chain.
#[derive(Debug, Clone, Copy)]
struct Journaled {
    record: u64,
    seat: Seat,
    head: DurableHead,
}

impl DurableHeads {
    /// No head of any of `seats` seats.
    pub(crate) fn new(seats: usize) -> Self {
        Self {
            heads: vec![None; seats],
            journaled: VecDeque::new(),
        }
    }

    /// Notes the packet of `seat` the journal holds as record `record`, which leaves its chain at
    /// `head`, once its record is synced.
    pub(crate) fn journaled(&mut self, record: u64, seat: Seat, head: DurableHead) {
        debug_assert!(
            self.journaled
                .back()
                .is_none_or(|last| last.record < record),
            "records are journaled in order"
        );
        self.journaled.push_back(Journaled { record, seat, head });
    }

    /// Notes where each chain stands once the journal's first `durable` records are synced.
    pub(crate) fn advance(&mut self, durable: u64) {
        while let Some(journaled) = self.journaled.front()
            && journaled.record < durable
        {
            self.heads[journaled.seat.index()] = Some(journaled.head);
            self.journaled.pop_front();
        }
    }

    /// Where `seat`'s chain stands in the synced records.
    pub(crate) fn get(&self, seat: Seat) -> Option<DurableHead> {
        self.heads.get(seat.index()).copied().flatten()
    }

    /// Sets where `seat`'s chain stands durably, as a journal read back from the disk holds it.
    pub(crate) fn set(&mut self, seat: Seat, head: Option<DurableHead>) {
        self.heads[seat.index()] = head;
    }

    /// Forgets where every chain stands durably, and every packet not synced yet: the log went
    /// back to a save, where each chain stood before the packets the journal holds since.
    pub(crate) fn clear(&mut self) {
        self.heads.fill(None);
        self.journaled.clear();
    }

    /// Forgets where `seat`'s chain stands durably, and its packets not synced yet: a player took
    /// the seat, renewed their delegation, or left it.
    pub(crate) fn forget(&mut self, seat: Seat) {
        self.heads[seat.index()] = None;
        self.journaled.retain(|journaled| journaled.seat != seat);
    }
}

#[cfg(test)]
mod tests {
    use crate::delegation::delegation_id::DelegationId;
    use crate::input_hash::InputHash;

    use super::*;

    #[test]
    fn a_head_is_durable_once_its_record_is_synced_and_forgotten_with_its_seat() {
        // Seat 0's packets in records 2 and 5, seat 1's in record 3.
        let head = |seq| DurableHead {
            delegation: DelegationId::new([1; 32]),
            seq,
            head: InputHash::new([seq as u8; 32]),
        };
        let seat = Seat::new;
        let mut heads = DurableHeads::new(2);
        heads.journaled(2, seat(0), head(1));
        heads.journaled(3, seat(1), head(7));
        heads.journaled(5, seat(0), head(2));
        // Records 0 to 2 synced: seat 0 at its first packet; record 3 to 5 not yet.
        heads.advance(3);
        assert_eq!(
            [heads.get(seat(0)), heads.get(seat(1))],
            [Some(head(1)), None]
        );
        // Seat 1 forgotten before its record syncs: it stays without a head.
        heads.forget(seat(1));
        heads.advance(6);
        assert_eq!(
            [heads.get(seat(0)), heads.get(seat(1))],
            [Some(head(2)), None]
        );
        assert_eq!(heads.get(seat(2)), None);
    }
}
