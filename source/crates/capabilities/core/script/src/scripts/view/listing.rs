use crate::scripts::view::unit_row::UnitRow;

/// Which units a list query gives: the living alone, or all, the dead among them, as its name says
/// (08-script-api.md, A list gives the living).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listing {
    Living,
    All,
}

impl Listing {
    /// Whether it lists the unit of `row`.
    pub const fn lists(self, row: &UnitRow) -> bool {
        match self {
            Self::Living => row.alive,
            Self::All => true,
        }
    }
}
