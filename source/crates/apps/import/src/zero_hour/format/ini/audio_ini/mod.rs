use std::collections::BTreeMap;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, FlagSet, IniLine, IniText};

/// What the import reads of Zero Hour's audio INI files, as `AudioManager` reads them: the
/// `AudioEvent`s of `SoundEffects.ini` and `Voice.ini` and the `MusicTrack`s of `Music.ini`, each
/// by its name, its case kept, as `findAudioEventInfo` matches it; each starts from the fields of
/// `DefaultSoundEffect` or `DefaultMusicTrack` as they stand when it starts, and a name read again
/// starts again from them (`parseAudioEventDefinition`); and `AudioSettings.ini`'s settings.
#[derive(Debug, Default)]
pub(crate) struct AudioIni {
    events: BTreeMap<String, IniAudioEvent>,
    tracks: BTreeMap<String, IniTrack>,
    pub(crate) settings: AudioSettings,
}

/// An audio event's fields (`AudioEventInfo::m_audioEventInfo`): its file stems; `Volume`,
/// `VolumeShift` and `MinVolume` as fractions of 1; `PitchShift` as rates, `1 + percent / 100`;
/// `Delay` in milliseconds; `Limit`, 0 for none; `LoopCount`; its `Type` and `Control`; whether it
/// names `Attack`, `Decay` or a time of day's sounds; `MinRange` and `MaxRange`; and
/// `LowPassCutoff`, a fraction of 1, 0 for none.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct IniAudioEvent {
    pub(crate) sounds: Vec<String>,
    pub(crate) volume: f32,
    pub(crate) volume_shift: f32,
    pub(crate) min_volume: f32,
    pub(crate) pitch: [f32; 2],
    pub(crate) delay_ms: [i32; 2],
    pub(crate) limit: i32,
    pub(crate) loop_count: i32,
    pub(crate) types: SoundTypes,
    pub(crate) control: SoundControl,
    pub(crate) attack_or_decay: bool,
    pub(crate) time_of_day: bool,
    pub(crate) min_range: f32,
    pub(crate) max_range: f32,
    pub(crate) low_pass: f32,
}

/// An event's `Type` (`theSoundTypeNames`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SoundTypes {
    pub(crate) ui: bool,
    pub(crate) world: bool,
    pub(crate) shrouded: bool,
    pub(crate) global: bool,
    pub(crate) voice: bool,
    pub(crate) player: bool,
    pub(crate) allies: bool,
    pub(crate) enemies: bool,
    pub(crate) everyone: bool,
}

/// An event's `Control` (`theAudioControlNames`); `ALL` and `POSTDELAY` change no play the
/// game makes of a sound of one portion, so they are taken and not kept.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SoundControl {
    pub(crate) looped: bool,
    pub(crate) random: bool,
    pub(crate) interrupt: bool,
}

/// A music track: its file in `Data\Audio\Tracks\`, and its volume, a fraction of 1.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct IniTrack {
    pub(crate) file: String,
    pub(crate) volume: f32,
}

/// What the import reads of `AudioSettings.ini` (`AudioSettings`), each a fraction of 1 but the
/// distances: the ranges of a `GLOBAL` sound; the volumes of the flat sounds, the positional ones
/// and the music, and the flat ones' share past the positional ones, `Relative2DVolume`; the
/// microphone's height above the ground and its share of the way to the camera; and the zoom's
/// distances and the share of the positional volume it raises.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AudioSettings {
    pub(crate) global_range: [f32; 2],
    pub(crate) sound_volume: f32,
    pub(crate) sound_3d_volume: f32,
    pub(crate) music_volume: f32,
    pub(crate) relative_2d_volume: f32,
    pub(crate) microphone_height: f32,
    pub(crate) microphone_share: f32,
    pub(crate) zoom_range: [f32; 2],
    pub(crate) zoom_share: f32,
}

impl AudioIni {
    /// The event a default of every event's fields is named.
    const DEFAULT_EVENT: &str = "DefaultSoundEffect";
    /// The track a default of every track's fields is named.
    const DEFAULT_TRACK: &str = "DefaultMusicTrack";

    /// Reads one INI file of `AudioEvent` and `MusicTrack` blocks, passing over the others.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes).lines().blocks(BlockKinds::Any) {
            let block = block?;
            match block.head.word() {
                "audioevent" => {
                    let name = block.name()?.to_owned();
                    let mut event = self.events.get(Self::DEFAULT_EVENT).cloned();
                    let event = event.get_or_insert_default();
                    for line in &block.fields {
                        event.field(line)?;
                    }
                    self.events.insert(name, event.clone());
                }
                "musictrack" => {
                    let name = block.name()?.to_owned();
                    let mut track = self.tracks.get(Self::DEFAULT_TRACK).cloned();
                    let track = track.get_or_insert_default();
                    for line in &block.fields {
                        match line.word() {
                            "filename" => line.value()?.clone_into(&mut track.file),
                            "volume" => track.volume = percent(line.real()?),
                            _ => {}
                        }
                    }
                    self.tracks.insert(name, track.clone());
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Reads one `AudioSettings.ini`, its block's fields replacing those read before.
    pub(crate) fn read_settings(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        let settings = &mut self.settings;
        for block in IniText::of(bytes)
            .lines()
            .blocks(BlockKinds::One("audiosettings"))
        {
            for line in &block?.fields {
                let field = match line.word() {
                    "globalminrange" => &mut settings.global_range[0],
                    "globalmaxrange" => &mut settings.global_range[1],
                    "microphonedesiredheightaboveterrain" => &mut settings.microphone_height,
                    "zoommindistance" => &mut settings.zoom_range[0],
                    "zoommaxdistance" => &mut settings.zoom_range[1],
                    word => {
                        let field = match word {
                            "defaultsoundvolume" => &mut settings.sound_volume,
                            "default3dsoundvolume" => &mut settings.sound_3d_volume,
                            "defaultmusicvolume" => &mut settings.music_volume,
                            "relative2dvolume" => &mut settings.relative_2d_volume,
                            "microphonemaxpercentagebetweengroundandcamera" => {
                                &mut settings.microphone_share
                            }
                            "zoomsoundvolumepercentageamount" => &mut settings.zoom_share,
                            _ => continue,
                        };
                        *field = percent(line.real()?);
                        continue;
                    }
                };
                *field = line.real()?;
            }
        }
        Ok(())
    }

    /// The event `name`, its case as the game's; none for a name no file defines.
    pub(crate) fn event(&self, name: &str) -> Option<&IniAudioEvent> {
        self.events.get(name)
    }

    /// The track `name`; none for a name no file defines.
    pub(crate) fn track(&self, name: &str) -> Option<&IniTrack> {
        self.tracks.get(name)
    }
}

impl IniAudioEvent {
    /// Takes the line `line` of its block, as its parse table reads it; a field it does not read
    /// it passes over.
    fn field(&mut self, line: &IniLine<'_>) -> Result<(), IniError> {
        match line.word() {
            "sounds" => self.sounds = sounds(line),
            "attack" | "decay" => self.attack_or_decay |= !sounds(line).is_empty(),
            "soundsnight" | "soundsevening" | "soundsmorning" => {
                self.time_of_day |= !sounds(line).is_empty();
            }
            "volume" => self.volume = percent(line.real()?),
            "volumeshift" => self.volume_shift = percent(line.real()?),
            "minvolume" => self.min_volume = percent(line.real()?),
            "lowpasscutoff" => self.low_pass = percent(line.real()?),
            "pitchshift" => {
                let shift = |at| line.real_at(at).map(|shift| 1.0 + percent(shift));
                self.pitch = [shift(1)?, shift(2)?];
            }
            "delay" => self.delay_ms = [line.int_at(1)?, line.int_at(2)?],
            "limit" => self.limit = line.int()?,
            "loopcount" => self.loop_count = line.int()?,
            "minrange" => self.min_range = line.real()?,
            "maxrange" => self.max_range = line.real()?,
            "type" => line.set_flags(&mut self.types)?,
            "control" => line.set_flags(&mut self.control)?,
            _ => {}
        }
        Ok(())
    }
}

impl Default for AudioSettings {
    /// As `AudioSettings` starts: every number 0.
    fn default() -> Self {
        Self {
            global_range: [0.0; 2],
            sound_volume: 0.0,
            sound_3d_volume: 0.0,
            music_volume: 0.0,
            relative_2d_volume: 0.0,
            microphone_height: 0.0,
            microphone_share: 0.0,
            zoom_range: [0.0; 2],
            zoom_share: 0.0,
        }
    }
}

impl FlagSet for SoundTypes {
    fn clear(&mut self) {
        *self = Self::default();
    }

    fn set(&mut self, name: &str, on: bool) -> bool {
        let flag = match name.to_ascii_lowercase().as_str() {
            "ui" => &mut self.ui,
            "world" => &mut self.world,
            "shrouded" => &mut self.shrouded,
            "global" => &mut self.global,
            "voice" => &mut self.voice,
            "player" => &mut self.player,
            "allies" => &mut self.allies,
            "enemies" => &mut self.enemies,
            "everyone" => &mut self.everyone,
            _ => return false,
        };
        *flag = on;
        true
    }
}

impl FlagSet for SoundControl {
    fn clear(&mut self) {
        *self = Self::default();
    }

    fn set(&mut self, name: &str, on: bool) -> bool {
        let flag = match name.to_ascii_lowercase().as_str() {
            "loop" => &mut self.looped,
            "random" => &mut self.random,
            "interrupt" => &mut self.interrupt,
            "all" | "postdelay" => return true,
            _ => return false,
        };
        *flag = on;
        true
    }
}

/// `value` in percent as a fraction of 1, as `parsePercentToReal` reads `90` and `90%`.
fn percent(value: f32) -> f32 {
    value / 100.0
}

/// The stems of a field of sounds, as `parseSoundsList` reads them, cut at every space, tab, `,`
/// and `=`.
fn sounds(line: &IniLine<'_>) -> Vec<String> {
    let tokens = line.tokens[1..].iter().flat_map(|token| token.split(','));
    tokens
        .filter(|stem| !stem.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests;
