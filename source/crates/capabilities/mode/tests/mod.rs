//! The tests of the crate that build a whole match, which links the crate once.

#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod mode;
mod timers;
