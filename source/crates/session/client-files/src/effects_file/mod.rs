use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_values::PackagePath;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::client_file::{ClientFile, ClientFormat};
use crate::client_path;
use crate::effect_list::EffectList;
use crate::error::EffectsError;
use crate::particle_system::ParticleSystem;

/// A package's `client/effects.toml`: the particle systems and the effects of lists of them that
/// its looks name, by name; the length of the step every system counts in, in seconds; and the
/// most particles alive at once, past which a new one removes the oldest of a lower rank
/// (effects.md, Effects are a format of the engine).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EffectsFile {
    step: f32,
    cap: u32,
    systems: BTreeMap<DeclaredName, ParticleSystem>,
    lists: BTreeMap<DeclaredName, EffectList>,
}

impl EffectsFile {
    /// The effects of `systems` and `lists`, each step `step` seconds long, of at most `cap`
    /// particles; refused for a step not above 0 or not finite, a system or a list's entry of a
    /// number not finite or of keys out of order, or a name of a system it does not hold, as a
    /// read refuses it.
    pub fn new(
        step: f32,
        cap: u32,
        systems: BTreeMap<DeclaredName, ParticleSystem>,
        lists: BTreeMap<DeclaredName, EffectList>,
    ) -> Result<Self, EffectsError> {
        if !(step > 0.0 && step.is_finite()) {
            return Err(EffectsError::Step);
        }
        if let Some((name, _)) = systems.iter().find(|(_, system)| !system.holds()) {
            return Err(EffectsError::System(name.clone()));
        }
        let entries = lists
            .iter()
            .flat_map(|(name, list)| list.systems.iter().map(move |entry| (name, entry)));
        for (list, entry) in entries {
            if !entry.is_finite() {
                return Err(EffectsError::List(list.clone()));
            }
            if !systems.contains_key(&entry.system) {
                return Err(EffectsError::Unknown(entry.system.clone()));
            }
        }
        let named = systems.values().flat_map(ParticleSystem::names);
        if let Some(name) = named.into_iter().find(|name| !systems.contains_key(*name)) {
            return Err(EffectsError::Unknown(name.clone()));
        }
        Ok(Self {
            step,
            cap,
            systems,
            lists,
        })
    }

    /// Where a package holds it.
    pub fn path() -> PackagePath {
        client_path::parse("client/effects.toml")
    }

    /// The seconds each step lasts.
    pub const fn step(&self) -> f32 {
        self.step
    }

    /// The most particles alive at once.
    pub const fn cap(&self) -> u32 {
        self.cap
    }

    pub const fn systems(&self) -> &BTreeMap<DeclaredName, ParticleSystem> {
        &self.systems
    }

    pub const fn lists(&self) -> &BTreeMap<DeclaredName, EffectList> {
        &self.lists
    }
}

impl ClientFile for EffectsFile {
    const FORMAT: ClientFormat = ClientFormat::Toml;
}

/// A package's file is untrusted, so one `new` refuses fails to read.
impl<'de> Deserialize<'de> for EffectsFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            step: f32,
            cap: u32,
            #[serde(default)]
            systems: BTreeMap<DeclaredName, ParticleSystem>,
            #[serde(default)]
            lists: BTreeMap<DeclaredName, EffectList>,
        }
        let Fields {
            step,
            cap,
            systems,
            lists,
        } = Fields::deserialize(deserializer)?;
        Self::new(step, cap, systems, lists).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests;
