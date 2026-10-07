use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// The cells A* may expand next, in the order it takes them: by total, cost plus estimate, then
/// by estimate, then by number. The octile estimate is consistent, so the least total never
/// falls, and a step adds to a total at most a diagonal to the cost and a diagonal to the
/// estimate: every waiting total lies within `SPREAD` of the least. So the cells wait in a ring of
/// buckets by total, each a heap by estimate and number, smaller than one heap of all of them and
/// ordered by one number.
#[derive(Debug)]
pub(crate) struct OpenCells {
    buckets: Vec<BinaryHeap<Reverse<u64>>>,
    /// The total of the bucket the next cell comes from, the least of the search's totals from
    /// its first push on; `None` before it.
    least: Option<u32>,
    len: usize,
}

impl OpenCells {
    /// How far past the least total a waiting total may lie: a diagonal step's cost, 14, and as
    /// much again of estimate.
    const SPREAD: u32 = 28;
    /// The buckets of the ring, more than the totals from the least to `SPREAD` past it.
    const RING: u32 = 32;

    pub(crate) fn new() -> OpenCells {
        OpenCells {
            buckets: (0..OpenCells::RING).map(|_| BinaryHeap::new()).collect(),
            least: None,
            len: 0,
        }
    }

    pub(crate) fn clear(&mut self) {
        for bucket in &mut self.buckets {
            bucket.clear();
        }
        self.least = None;
        self.len = 0;
    }

    /// Adds `cell` at `total` and `estimate`: a total from the least to `SPREAD` past it.
    pub(crate) fn push(&mut self, total: u32, estimate: u32, cell: u32) {
        let least = *self.least.get_or_insert(total);
        debug_assert!(
            total >= least && total - least <= OpenCells::SPREAD,
            "a consistent estimate keeps every total near the least"
        );
        let key = u64::from(estimate) << 32 | u64::from(cell);
        self.buckets[(total % OpenCells::RING) as usize].push(Reverse(key));
        self.len += 1;
    }

    /// Takes the cell of the least total, then estimate, then number.
    pub(crate) fn pop(&mut self) -> Option<u32> {
        if self.len == 0 {
            return None;
        }
        let least = self
            .least
            .as_mut()
            .expect("a pushed cell set the least total");
        loop {
            let bucket = &mut self.buckets[(*least % OpenCells::RING) as usize];
            if let Some(Reverse(key)) = bucket.pop() {
                self.len -= 1;
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "the low 32 bits hold the cell"
                )]
                return Some(key as u32);
            }
            *least += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_come_out_by_total_then_estimate_then_number() {
        // A search's pushes: each step pops the least and pushes a few at most `SPREAD` past
        // its total, some at the same total, the widest estimates and numbers included. The ring
        // gives them back in the order one heap of (total, estimate, number) does.
        let mut open = OpenCells::new();
        let mut heap = BinaryHeap::new();
        let mut state = 0x0FE2_u64;
        let mut next = move |below: u32| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            u32::try_from(state >> 33).unwrap() % below
        };
        let push = |open: &mut OpenCells, heap: &mut BinaryHeap<_>, at: (u32, u32, u32)| {
            open.push(at.0, at.1, at.2);
            heap.push(Reverse(at));
        };
        push(&mut open, &mut heap, (100, 7, u32::MAX));
        for _ in 0..5000 {
            let Some(Reverse((total, _, cell))) = heap.pop() else {
                break;
            };
            assert_eq!(open.pop(), Some(cell));
            for _ in 0..next(4) {
                let estimate = if next(9) == 0 { u32::MAX } else { next(50) };
                let at = (total + next(OpenCells::SPREAD + 1), estimate, next(1000));
                push(&mut open, &mut heap, at);
            }
        }
        while let Some(Reverse((_, _, cell))) = heap.pop() {
            assert_eq!(open.pop(), Some(cell));
        }
        assert_eq!(open.pop(), None);
        // A cleared ring starts again at any total: a new search's, below the last.
        open.clear();
        open.push(9, 1, 2);
        open.clear();
        open.push(3, 0, 5);
        assert_eq!(open.pop(), Some(5));
        assert_eq!(open.pop(), None);
    }
}
