use serde::{Deserialize, Serialize};

/// How a unit moves through a crowd (04-capabilities/navigation.md, A unit moves by its kind of
/// motion): it steps, as infantry, and collision parts it from another any way, or it drives, as a
/// vehicle, and collision moves it only along its facing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gait {
    Step,
    Drive,
}

impl Gait {
    /// Whether a walker of this gait may move a unit of `other` aside on a way no unit of its team
    /// blocks: one that steps moves no unit that drives, as Zero Hour's `moveAllies` lets no
    /// infantry push a vehicle off a path no ally blocks (entries-and-blocking.md, A walker that
    /// steps moves no unit that drives aside).
    pub(crate) const fn moves(self, other: Self) -> bool {
        !matches!((self, other), (Self::Step, Self::Drive))
    }
}
