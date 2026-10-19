use crate::session_log::at;

/// Byte strings, each in one piece, in pages that never move: a page holds `PAGE` bytes, or one
/// string longer than that alone, so a push never copies the bytes before it, and each string
/// reads as one slice.
#[derive(Debug, Default)]
pub(crate) struct PagedBytes {
    pages: Vec<Vec<u8>>,
}

/// Where a string sits: its page, its start in it, and its length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BytesAt {
    page: u32,
    start: u32,
    len: u32,
}

/// Where the strings end: the pages they take, and the bytes of the last one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BytesEnd {
    pages: u32,
    last: u32,
}

/// Bytes in a page.
const PAGE: usize = 64 * 1024;

impl PagedBytes {
    /// Adds `bytes` after the last page's, or at the start of a new page when they do not fit
    /// in the room it has left.
    pub(crate) fn push(&mut self, bytes: &[u8]) -> BytesAt {
        let fits = self
            .pages
            .last()
            .is_some_and(|page| page.capacity() - page.len() >= bytes.len());
        if !fits {
            self.pages.push(Vec::with_capacity(PAGE.max(bytes.len())));
        }
        let page = self.pages.last_mut().expect("a page with room");
        let start = page.len();
        page.extend_from_slice(bytes);
        let narrow = |value: usize| u32::try_from(value).expect("a log's positions fit u32");
        BytesAt {
            page: narrow(self.pages.len() - 1),
            start: narrow(start),
            len: narrow(bytes.len()),
        }
    }

    pub(crate) fn get(&self, place: BytesAt) -> &[u8] {
        let start = at(place.start);
        &self.pages[at(place.page)][start..start + at(place.len)]
    }

    /// Where the strings end now.
    pub(crate) fn end(&self) -> BytesEnd {
        let narrow = |value: usize| u32::try_from(value).expect("a log's positions fit u32");
        BytesEnd {
            pages: narrow(self.pages.len()),
            last: narrow(self.pages.last().map_or(0, Vec::len)),
        }
    }

    /// Forgets the pages before the page of `place`, or every page before `end`'s last when
    /// `place` is none: their memory goes, and each string they do not hold keeps its place.
    pub(crate) fn forget_before(&mut self, place: Option<BytesAt>) {
        let page = place.map_or_else(
            || at(self.end().pages).saturating_sub(1),
            |place| at(place.page),
        );
        for forgotten in self.pages.iter_mut().take(page) {
            *forgotten = Vec::new();
        }
    }

    /// Drops the strings past `end`, as a load drops what came after its save.
    pub(crate) fn truncate(&mut self, end: BytesEnd) {
        self.pages.truncate(at(end.pages));
        if let Some(last) = self.pages.last_mut() {
            last.truncate(at(end.last));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use super::*;

    #[test]
    fn each_string_reads_whole_and_a_push_never_moves_the_ones_before() {
        let mut bytes = PagedBytes::default();
        let empty = bytes.push(b"");
        let first = bytes.push(b"abc");
        let at = bytes.pages[0].as_ptr();
        // 65 533 bytes fill the first page to 65 536; one more byte starts the second page; a
        // string longer than a page takes a page of its own, of its length.
        let fill = bytes.push(&vec![7; PAGE - 3]);
        let next = bytes.push(b"z");
        let long = bytes.push(&vec![9; PAGE + 1]);
        let after = bytes.push(b"q");
        assert_eq!(bytes.pages.len(), 4);
        assert_eq!(bytes.pages[2].capacity(), PAGE + 1);
        assert!(ptr::eq(at, bytes.pages[0].as_ptr()));
        assert_eq!(bytes.get(empty), b"");
        assert_eq!(bytes.get(first), b"abc");
        assert_eq!(bytes.get(fill).len(), PAGE - 3);
        assert_eq!((next.page, next.start), (1, 0));
        assert_eq!(bytes.get(next), b"z");
        assert_eq!(bytes.get(long), vec![9; PAGE + 1]);
        // Only the last page takes more: the long string's is full, so "q" starts a fourth.
        assert_eq!((after.page, after.start), (3, 0));

        // Cut back to where "z" ends, the long string goes, and a push goes on after "z" in its
        // page while it fits.
        let mut cut = PagedBytes::default();
        cut.push(&vec![7; PAGE - 3]);
        cut.push(b"z");
        let end = cut.end();
        cut.push(&vec![9; PAGE + 1]);
        cut.truncate(end);
        assert_eq!(cut.end(), end);
        let fits = cut.push(b"y");
        assert_eq!((fits.page, fits.start), (0, PAGE as u32 - 2));
        let past = cut.push(b"yyy");
        assert_eq!((past.page, past.start), (1, 0));
        // Forgotten before "yyy", the first page goes, and "yyy" still reads.
        cut.forget_before(Some(past));
        assert_eq!(cut.pages[0].capacity(), 0);
        assert_eq!(cut.get(past), b"yyy");
        bytes.forget_before(None);
        assert_eq!(bytes.get(after), b"q");
    }
}
