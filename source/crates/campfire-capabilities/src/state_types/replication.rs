use campfire_sim::{Position, SimComponent};

use crate::state_types::data_kind::DataKind;
use crate::state_types::sending::{Sending, SentPredicted};

/// A state type of a capability: a `SimComponent`, the kind its clients receive it as, and how
/// it is sent. Neither has a default, so a type in a capability's list that lacks one does not
/// compile, and a way to send it that its type does not allow, as a sent-once type that is not
/// immutable, does not either.
pub trait Replication: SimComponent {
    const KIND: DataKind;
    type Sending: Sending<Self>;
}

/// `sim`'s own place of a unit, which `sim` holds no kind of, as it holds no genre.
impl Replication for Position {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
