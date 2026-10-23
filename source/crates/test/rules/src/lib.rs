//! The workspace's structural rules (02-engine-core.md, Structural rules): each test reads the
//! workspace's sources, manifests, `clippy.toml` or design documents, and fails on what breaks a
//! rule, beside a test of its own check on samples that break it. Each module is a test's, gated at
//! its declaration, as the rules read a gate there.

#[cfg(test)]
mod allowlist;
#[cfg(test)]
mod bans;
#[cfg(test)]
mod capabilities;
#[cfg(test)]
mod cfg_condition;
#[cfg(test)]
mod citations;
#[cfg(test)]
mod cycles;
#[cfg(test)]
mod dev_dependencies;
#[cfg(test)]
mod errors;
#[cfg(test)]
mod gated_items;
#[cfg(test)]
mod manifest;
#[cfg(test)]
mod manifests;
#[cfg(test)]
mod named_path;
#[cfg(test)]
mod platform;
#[cfg(test)]
mod re_exports;
#[cfg(test)]
mod source_file;
#[cfg(test)]
mod source_files;
#[cfg(test)]
mod suffixes;
#[cfg(test)]
mod test_homes;
#[cfg(test)]
mod test_modules;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod type_names;
#[cfg(test)]
mod unsafe_code;
#[cfg(test)]
mod workspace;
#[cfg(test)]
mod world;
