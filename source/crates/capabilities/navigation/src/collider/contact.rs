/// Two colliders whose bodies overlap, by their indices, the first the lower.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Contact {
    pub(crate) first: usize,
    pub(crate) second: usize,
}
