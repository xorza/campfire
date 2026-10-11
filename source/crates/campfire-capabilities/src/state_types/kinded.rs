use campfire_sim::{Position, SimComponent};

use crate::state_types::data_kind::DataKind;

/// A state type of a capability: a `SimComponent` and the kind its clients receive it as. No
/// kind has a default, so a type in a capability's list with none does not compile.
pub trait Kinded: SimComponent {
    const KIND: DataKind;
}

/// `sim`'s own place of a unit, which `sim` holds no kind of, as it holds no genre.
impl Kinded for Position {
    const KIND: DataKind = DataKind::Unit;
}
