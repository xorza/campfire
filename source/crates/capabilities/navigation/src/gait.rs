use serde::{Deserialize, Serialize};

/// How a unit moves through a crowd (04-capabilities/navigation.md, A unit moves by its kind of
/// motion): it steps, as infantry, and takes a push any way, or it drives, as a vehicle, and moves
/// only along its facing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gait {
    Step,
    Drive,
}
