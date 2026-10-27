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
use crate::track_marks::TrackMarks;
use crate::tread_dust::TreadDust;
use crate::wreck_ground::WreckGround;

/// A package's `client/effects.toml`: the particle systems and the effects of lists of them that
/// its looks name, by name; the length of the step every system counts in, in seconds; the most
/// particles alive at once, past which a new one removes the oldest of a lower rank
/// (effects.md, Effects are a format of the engine); how a wreck meets the ground (wrecks.md,
/// Wrecks move by the client's step); and how a tread throws its dust and lays its track
/// (wrecks.md, A tread throws dust while it drives; wrecks.md, A tread lays track marks).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EffectsFile {
    step: f32,
    #[serde(skip_serializing_if = "WreckGround::is_none")]
    wrecks: WreckGround,
    #[serde(skip_serializing_if = "TreadDust::is_none")]
    treads: TreadDust,
    #[serde(skip_serializing_if = "TrackMarks::is_none")]
    tracks: TrackMarks,
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
            wrecks: WreckGround::NONE,
            treads: TreadDust::NONE,
            tracks: TrackMarks::NONE,
            cap,
            systems,
            lists,
        })
    }

    /// It, its wrecks meeting the ground as `wrecks` says; refused for a ground that does not
    /// hold.
    pub fn with_wrecks(mut self, wrecks: WreckGround) -> Result<Self, EffectsError> {
        if !wrecks.holds() {
            return Err(EffectsError::Wrecks);
        }
        self.wrecks = wrecks;
        Ok(self)
    }

    /// How a wreck meets the ground.
    pub const fn wrecks(&self) -> WreckGround {
        self.wrecks
    }

    /// It, its treads throwing their dust as `treads` says and laying their tracks as `tracks`
    /// says; refused for either that does not hold.
    pub fn with_treads(
        mut self,
        treads: TreadDust,
        tracks: TrackMarks,
    ) -> Result<Self, EffectsError> {
        if !treads.holds() {
            return Err(EffectsError::Treads);
        }
        if !tracks.holds() {
            return Err(EffectsError::Tracks);
        }
        self.treads = treads;
        self.tracks = tracks;
        Ok(self)
    }

    /// How a tread throws its dust.
    pub const fn treads(&self) -> TreadDust {
        self.treads
    }

    /// How a tread lays its track.
    pub const fn tracks(&self) -> TrackMarks {
        self.tracks
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
            #[serde(default)]
            wrecks: WreckGround,
            #[serde(default)]
            treads: TreadDust,
            #[serde(default)]
            tracks: TrackMarks,
            cap: u32,
            #[serde(default)]
            systems: BTreeMap<DeclaredName, ParticleSystem>,
            #[serde(default)]
            lists: BTreeMap<DeclaredName, EffectList>,
        }
        let Fields {
            step,
            wrecks,
            treads,
            tracks,
            cap,
            systems,
            lists,
        } = Fields::deserialize(deserializer)?;
        let file = Self::new(step, cap, systems, lists).map_err(D::Error::custom)?;
        let file = file.with_wrecks(wrecks).map_err(D::Error::custom)?;
        file.with_treads(treads, tracks).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests;
