use campfire_sim::StableId;
use campfire_values::{ActionId, Rank};

use crate::lifetime::Hold;

/// Who applies a modifier: its source, none from the mode; the ability that applies it, at
/// `rank`, rank 1 with none; and what holds it, a passive or an aura, an area or a player, or
/// none for an application of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Applier {
    pub(crate) source: Option<StableId>,
    pub(crate) action: Option<ActionId>,
    pub(crate) rank: Rank,
    pub(crate) hold: Option<Hold>,
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_sim::StableId;
    use campfire_values::{ActionId, Rank};

    use crate::applier::Applier;
    use crate::lifetime::Hold;

    impl Applier {
        /// The mode's application, from no source, of no action, at rank 1, held by nothing.
        pub const MODE: Self = Self {
            source: None,
            action: None,
            rank: Rank::FIRST,
            hold: None,
        };

        /// An application from `source`, of no action, at rank 1, held by nothing.
        pub const fn from_unit(source: StableId) -> Self {
            Self {
                source: Some(source),
                ..Self::MODE
            }
        }

        /// The same application, by `action` at `rank`.
        #[must_use]
        pub const fn by(self, action: ActionId, rank: Rank) -> Self {
            Self {
                action: Some(action),
                rank,
                ..self
            }
        }

        /// The same application, held as a passive.
        #[must_use]
        pub const fn passive(self) -> Self {
            Self {
                hold: Some(Hold::Passive),
                ..self
            }
        }
    }
}
