use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_values::PackagePath;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::client_file::{ClientFile, ClientFormat};
use crate::client_path;
use crate::error::SoundFileError;

/// A mode's `client/sound.toml`: the volume of each class of sound, where the listener stands, how
/// a near camera raises the positional sounds, and the music (02-engine-core.md, The listener is
/// the game's microphone; 02-engine-core.md, Music by a choice). A mode of no file plays each class
/// at its full volume, hears at the camera, raises nothing, and plays no music.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SoundFile {
    pub volumes: SoundVolumes,
    /// Where the listener stands; none for the camera.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listener: Option<Listener>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoom: Option<ZoomBoost>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub music: Option<Music>,
}

/// The volume of each class of sound, from 0 to 1: the sounds that play at a place, those that
/// play flat, and the music.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundVolumes {
    pub positional: f32,
    pub flat: f32,
    pub music: f32,
}

/// Where the listener stands: from the camera's point on the ground toward the camera, `height`
/// meters above that point or `share` of the way, whichever is nearer, as `AudioManager::update`
/// places Zero Hour's microphone.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Listener {
    pub height: f32,
    pub share: f32,
}

/// How a near camera raises the positional sounds: they play at `1 − boost` of their volume while
/// the camera stands `far` meters or more from the listener, rising linearly to all of it at `near`
/// and nearer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZoomBoost {
    pub near: f32,
    pub far: f32,
    pub boost: f32,
}

/// The music: the mode's choice whose pick of the player's own names its playlist, and the
/// playlist of each of the choice's values, its tracks played in turn and again from the first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Music {
    pub choice: DeclaredName,
    pub playlists: BTreeMap<DeclaredName, Vec<PackagePath>>,
}

impl SoundFile {
    /// Whether it holds; refused for a volume or a share past 0 to 1, a height below 0, a zoom
    /// whose far is not past its near, or a playlist of no track, each number finite.
    pub fn check(&self) -> Result<(), SoundFileError> {
        let share = |share: f32| (0.0..=1.0).contains(&share);
        let SoundVolumes {
            positional,
            flat,
            music,
        } = self.volumes;
        if ![positional, flat, music].into_iter().all(share) {
            return Err(SoundFileError::Volume);
        }
        let listener = |at: Listener| at.height >= 0.0 && at.height.is_finite() && share(at.share);
        if !self.listener.is_none_or(listener) {
            return Err(SoundFileError::Listener);
        }
        let zoom = |zoom: ZoomBoost| {
            zoom.near >= 0.0 && zoom.far > zoom.near && zoom.far.is_finite() && share(zoom.boost)
        };
        if !self.zoom.is_none_or(zoom) {
            return Err(SoundFileError::Zoom);
        }
        let playlists = self.music.iter().flat_map(|music| &music.playlists);
        if let Some((value, _)) = playlists.into_iter().find(|(_, tracks)| tracks.is_empty()) {
            return Err(SoundFileError::Playlist(value.clone()));
        }
        Ok(())
    }

    /// Where a mode's package holds it.
    pub fn path() -> PackagePath {
        client_path::parse("client/sound.toml")
    }
}

impl Default for SoundVolumes {
    /// Each class at its full volume.
    fn default() -> Self {
        Self {
            positional: 1.0,
            flat: 1.0,
            music: 1.0,
        }
    }
}

impl ClientFile for SoundFile {
    const FORMAT: ClientFormat = ClientFormat::Toml;
}

/// A package's file is untrusted, so one `new` refuses fails to read.
impl<'de> Deserialize<'de> for SoundFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(default)]
            volumes: SoundVolumes,
            #[serde(default)]
            listener: Option<Listener>,
            #[serde(default)]
            zoom: Option<ZoomBoost>,
            #[serde(default)]
            music: Option<Music>,
        }
        let Fields {
            volumes,
            listener,
            zoom,
            music,
        } = Fields::deserialize(deserializer)?;
        let file = Self {
            volumes,
            listener,
            zoom,
            music,
        };
        file.check().map_err(D::Error::custom)?;
        Ok(file)
    }
}

#[cfg(test)]
mod tests;
