use campfire_math::Num;
use thiserror::Error;

/// A number that is no radius: below 0, or past `MAX_BOUND`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("{0} is no radius: below 0 or past 2,048 m")]
pub struct NotRadius(pub Num);
