//! The test stepper of the stack's layers: a server app and client apps linked over the emulated
//! network, in plain packet sessions or sealed ones of the UDP protocol, on one virtual time. Each
//! layer's `internals` gives its stack on the one below as a [`Stack`] (13-net-stack/testing.md,
//! The stepper lives in `net-stepper`).

mod app_id;
mod app_schedule;
mod client_kind;
mod connection_stack;
mod fixed_run;
mod remote_client;
mod stack;
mod stepper;
mod stepper_setup;

pub use crate::app_id::AppId;
pub use crate::client_kind::ClientKind;
pub use crate::connection_stack::ConnectionStack;
pub use crate::stack::Stack;
pub use crate::stepper::{StepOrder, Stepper};
pub use crate::stepper_setup::{Init, StepperSetup};
