use std::ops::{Index, Range};

/// A list that grows a page at a time: each page holds `PAGE` items and never moves, so a push
/// never copies the items before it, as a `Vec` that doubles copies all of them.
#[derive(Debug)]
pub(crate) struct PagedVec<T> {
    pages: Vec<Vec<T>>,
    len: usize,
}

/// Items in a page: a log's buffers then grow by pages of tens of KiB.
const PAGE: usize = 4096;

impl<T> Default for PagedVec<T> {
    fn default() -> PagedVec<T> {
        PagedVec {
            pages: Vec::new(),
            len: 0,
        }
    }
}

impl<T> PagedVec<T> {
    pub(crate) const fn len(&self) -> usize {
        self.len
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(crate) fn push(&mut self, item: T) {
        if self.len.is_multiple_of(PAGE) {
            self.pages.push(Vec::with_capacity(PAGE));
        }
        let page = self.pages.last_mut().expect("a page with room");
        debug_assert!(page.len() < PAGE, "a page never grows past its capacity");
        page.push(item);
        self.len += 1;
    }

    pub(crate) fn get(&self, at: usize) -> Option<&T> {
        self.pages.get(at / PAGE)?.get(at % PAGE)
    }

    pub(crate) fn last(&self) -> Option<&T> {
        self.len.checked_sub(1).and_then(|at| self.get(at))
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        self.pages.iter().flatten()
    }

    /// The items at `range`, in order.
    pub(crate) fn range(&self, range: Range<usize>) -> impl Iterator<Item = &T> {
        assert!(range.end <= self.len, "a range within the list");
        range.map(|at| &self[at])
    }
}

impl<T> Index<usize> for PagedVec<T> {
    type Output = T;

    fn index(&self, at: usize) -> &T {
        &self.pages[at / PAGE][at % PAGE]
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
        assert!(list.is_empty());
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
    }
}
