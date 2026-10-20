use std::fmt::Debug;

use serde::de::{DeserializeOwned, Error};
use serde::{Deserialize, Deserializer, Serialize};

use crate::error::PatchError;

/// A data type the engine's tools write in part, over the data it lies on (03-game-scripting.md,
/// Overrides by load order): its patch is the all-`Option` twin of its fields, and the one list of
/// them, as its read goes through the patch and then checks it (03-game-scripting.md, A
/// section has a partial form).
pub trait Patched: Sized {
    type Patch: Debug + Default + Serialize + DeserializeOwned;

    /// The value `patch` holds whole; an error for a field it lacks that the value needs, and for
    /// what the value's rules refuse.
    fn from_patch(patch: Self::Patch) -> Result<Self, PatchError>;

    /// Reads the value through its patch, as its `Deserialize` does.
    fn read_patch<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let patch = Self::Patch::deserialize(deserializer)?;
        Self::from_patch(patch).map_err(|error| match error {
            PatchError::Missing(field) => D::Error::missing_field(field),
            PatchError::Refused(rule) => D::Error::custom(rule),
        })
    }
}
