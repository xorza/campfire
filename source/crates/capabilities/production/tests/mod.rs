//! The tests of the crate that build a whole match, which links the crate once.

#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

mod gatherer;
mod production;
