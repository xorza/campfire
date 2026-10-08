use crate::mode::mode_books::ModeBooks;
use crate::mode::mode_units::ModeUnits;

/// What the mode's install takes from the books: its unit types, avatars and loadout, and the
/// books of its own rules.
#[derive(Debug)]
pub struct ModeInputs {
    pub units: ModeUnits,
    pub books: ModeBooks,
}
