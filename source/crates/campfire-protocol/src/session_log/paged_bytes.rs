/// Byte strings, each in one piece, in pages that never move: a page holds `PAGE` bytes, or one
/// string longer than that alone, so a push never copies the bytes before it, and each string
/// reads as one slice.
#[derive(Debug, Default)]
pub(crate) struct PagedBytes {
    pages: Vec<Vec<u8>>,
    /// The bytes held, without the room a page left at its end.
    len: usize,
}

/// Where a string sits: its page, its start in it, and its length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BytesAt {
    page: u32,
    start: u32,
    len: u32,
}

/// Bytes in a page.
const PAGE: usize = 64 * 1024;

impl PagedBytes {
    /// The bytes held.
    pub(crate) const fn len(&self) -> usize {
        self.len
    }

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
        self.len += bytes.len();
        let narrow = |value: usize| u32::try_from(value).expect("a log's positions fit u32");
        BytesAt {
            page: narrow(self.pages.len() - 1),
            start: narrow(start),
            len: narrow(bytes.len()),
        }
    }

    pub(crate) fn get(&self, at: BytesAt) -> &[u8] {
        let start = at.start as usize;
        &self.pages[at.page as usize][start..start + at.len as usize]
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
        assert_eq!(bytes.len(), 3 + (PAGE - 3) + 1 + (PAGE + 1) + 1);
    }
}
