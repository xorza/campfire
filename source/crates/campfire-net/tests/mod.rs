#![expect(
    clippy::disallowed_methods,
    reason = "a test makes and removes the files of its fixtures"
)]
mod bots;
mod checkpoints;
mod fog;
mod lane;
mod local_server;
mod pace;
mod prototype;
mod receipts;
mod rejoin;
mod restore;
mod saves;
mod scenario;
mod scratch;
