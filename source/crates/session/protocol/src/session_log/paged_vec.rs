use std::ops::{Index, Range};

/// A list that grows a page at a time: each page holds `PAGE` items and never moves, so a push
/// never copies the items before it, as a `Vec` that doubles copies all of them. It may forget
/// the pages before an item, which then keeps its index, as a log forgets what its journal holds.
#[derive(Debug)]
pub(crate) struct PagedVec<T> {
    pages: Vec<Page<T>>,
    len: usize,
}

/// A page's items, after the first `forgotten` of its places, whose items it no longer holds.
#[derive(Debug)]
struct Page<T> {
    forgotten: usize,
    items: Vec<T>,
}

/// Items in a page: a log's buffers then grow by pages of tens of KiB.
const PAGE: usize = 4096;

impl<T> Default for PagedVec<T> {
    fn default() -> Self {
        Self {
            pages: Vec::new(),
            len: 0,
        }
    }
}

impl<T> PagedVec<T> {
    /// A list of `len` places and no item, as one that forgot every item before `len`: a push
    /// goes on at `len`.
    pub(crate) fn starting_at(len: usize) -> Self {
        let pages = (0..len.div_ceil(PAGE))
            .map(|page| {
                let forgotten = (len - page * PAGE).min(PAGE);
                Page {
                    forgotten,
                    items: Vec::with_capacity(PAGE - forgotten),
                }
            })
            .collect();
        Self { pages, len }
    }

    pub(crate) const fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn push(&mut self, item: T) {
        if self.len.is_multiple_of(PAGE) {
            self.pages.push(Page {
                forgotten: 0,
                items: Vec::with_capacity(PAGE),
            });
        }
        let page = self.pages.last_mut().expect("a page with room");
        debug_assert!(
            page.forgotten + page.items.len() < PAGE,
            "a page never grows past its capacity"
        );
        page.items.push(item);
        self.len += 1;
    }

    /// The item at `at`; none past the last, and none it forgot.
    pub(crate) fn get(&self, at: usize) -> Option<&T> {
        let page = self.pages.get(at / PAGE)?;
        page.items.get((at % PAGE).checked_sub(page.forgotten)?)
    }

    pub(crate) fn last(&self) -> Option<&T> {
        self.get(self.len.checked_sub(1)?)
    }

    /// Every item, in order; a list that forgot none.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        debug_assert!(
            self.holds_all(),
            "a list that forgot items iterates none of them"
        );
        self.pages.iter().flat_map(|page| &page.items)
    }

    /// Whether it forgot no item.
    pub(crate) fn holds_all(&self) -> bool {
        self.pages.iter().all(|page| page.forgotten == 0)
    }

    /// Forgets the pages that hold only items before `at`, whose memory goes; every item from
    /// `at` on keeps its index.
    pub(crate) fn forget_before(&mut self, at: usize) {
        debug_assert!(at <= self.len, "a list forgets the items it holds");
        for page in self.pages.iter_mut().take(at / PAGE) {
            *page = Page {
                forgotten: PAGE,
                items: Vec::new(),
            };
        }
    }

    /// Drops the items from `len` on, as a load drops what came after its save; the items it
    /// forgot stay forgotten, and a push goes on at `len`.
    pub(crate) fn truncate(&mut self, len: usize) {
        debug_assert!(len <= self.len, "a list drops only items it holds");
        self.pages.truncate(len.div_ceil(PAGE));
        let kept = len % PAGE;
        if let Some(last) = self.pages.last_mut()
            && kept > 0
        {
            if let Some(held) = kept.checked_sub(last.forgotten) {
                last.items.truncate(held);
            } else {
                last.items.clear();
                last.forgotten = kept;
            }
        }
        self.len = len;
    }

    /// The items at `range`, in order.
    pub(crate) fn range(&self, range: Range<usize>) -> impl Iterator<Item = &T> {
        debug_assert!(range.end <= self.len, "a range within the list");
        range.map(|at| &self[at])
    }
}

impl<T> Index<usize> for PagedVec<T> {
    type Output = T;

    fn index(&self, at: usize) -> &T {
        self.get(at).expect("an item the list holds")
    }
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use super::*;

    #[test]
    fn a_push_never_moves_the_items_before_it() {
        // Two pages and one item: 4096 + 4096 + 1; the first item stays where it was put.
        let mut list = PagedVec::default();
        assert_eq!(list.len(), 0);
        list.push(0_u32);
        let first = ptr::from_ref(&list[0]);
        for value in 1..=2 * 4096 {
            list.push(value);
        }
        assert_eq!(list.len(), 8193);
        assert!(ptr::eq(first, ptr::from_ref(&list[0])));
        assert_eq!(list.pages.len(), 3);
        assert_eq!((list[4095], list[4096], list[8192]), (4095, 4096, 8192));
        assert_eq!(list.last(), Some(&8192));
        assert_eq!(list.get(8193), None);
        assert!(list.iter().copied().eq(0..=8192));
        assert!(list.range(4094..4098).copied().eq(4094..4098));
        assert_eq!(PagedVec::<u32>::default().last(), None);

        // Forgotten before item 5000, the first page goes, and the items from the second keep
        // their places.
        list.forget_before(5000);
        assert_eq!(list.pages[0].items.capacity(), 0);
        assert!(!list.holds_all());
        assert_eq!((list[4096], list[5000], list[8192]), (4096, 5000, 8192));
        assert_eq!(list.get(10), None);
        // Cut back to 4100 items, the third page goes and the second keeps 4 items; a push goes
        // on at 4100.
        list.truncate(4100);
        assert_eq!((list.len(), list.pages.len()), (4100, 2));
        list.push(9);
        assert_eq!((list[4099], list[4100], list.last()), (4099, 9, Some(&9)));
        // Cut back to a page's end, the list holds its first page whole.
        let mut whole = PagedVec::default();
        for value in 0..4097_u32 {
            whole.push(value);
        }
        whole.truncate(4096);
        assert_eq!((whole.pages.len(), whole.pages[0].items.len()), (1, 4096));
        whole.push(7);
        assert_eq!(whole[4096], 7);
        // Cut back into a page it forgot, a push still takes its index.
        whole.forget_before(4097);
        whole.truncate(10);
        whole.push(3);
        assert_eq!(
            (whole.len(), whole.get(10), whole.get(9)),
            (11, Some(&3), None)
        );
        // Started at 4097, a list holds no item before it, and its pushes go on there: the
        // second page has 4095 places left, so 4095 pushes fill it and the next starts a third.
        let mut started = PagedVec::starting_at(4097);
        assert_eq!(
            (started.len(), started.get(4096), started.last()),
            (4097, None, None)
        );
        for value in 4097..8193_u32 {
            started.push(value);
        }
        assert_eq!(started.pages.len(), 3);
        assert_eq!(
            (started[4097], started[8191], started[8192]),
            (4097, 8191, 8192)
        );
        assert!(!started.holds_all());
        let empty = PagedVec::<u32>::starting_at(0);
        assert_eq!((empty.len(), empty.pages.len()), (0, 0));
    }
}
