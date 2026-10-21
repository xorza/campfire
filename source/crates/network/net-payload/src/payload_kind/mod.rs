/// The 2 bits of a payload's kind, which the packet session carries in its packet's header and does
/// not read: the layer above names its payload formats with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PayloadKind(u8);

impl PayloadKind {
    /// The number of kinds 2 bits name.
    pub const COUNT: u8 = 4;

    /// The kind `bits` names, below [`Self::COUNT`].
    pub const fn new(bits: u8) -> Self {
        assert!(bits < Self::COUNT, "a payload kind has 2 bits");
        Self(bits)
    }

    pub const fn bits(self) -> u8 {
        self.0
    }
}

#[cfg(test)]
mod tests;
