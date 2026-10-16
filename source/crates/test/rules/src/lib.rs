//! The workspace's structural rules (02-engine-core.md, Structural rules): each test reads the
//! workspace's sources, manifests, `clippy.toml` or design documents, and fails on what breaks a
//! rule, beside a test of its own check on samples that break it. Each module is a test's, gated at
//! its declaration, as the rules read a gate there.

#[cfg(test)]
mod bans;
#[cfg(test)]
mod citations;
#[cfg(test)]
mod cycles;
#[cfg(test)]
mod errors;
#[cfg(test)]
mod manifests;
#[cfg(test)]
mod platform;
#[cfg(test)]
mod re_exports;
#[cfg(test)]
mod workspace;
#[cfg(test)]
mod world;
