use std::collections::BTreeMap;
use std::num::NonZeroU16;

use campfire_client_files::{
    Hearers, Listener, Music, SoundEvent, SoundFile, SoundLimit, SoundPick, SoundRange,
    SoundVolumes, UnitVoices, ZoomBoost,
};
use campfire_common::DeclaredName;
use campfire_values::PackagePath;

use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::dropped::{Dropped, Losses};
use crate::zero_hour::error::{NameClash, NameError, ZeroHourError};
use crate::zero_hour::format::ini::audio_ini::{AudioIni, AudioSettings, IniAudioEvent};
use crate::zero_hour::format::ini::object_ini::ini_voices::IniVoices;
use crate::zero_hour::format::wav::{WavEncoding, WavFile};
use crate::zero_hour::game_path;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::sound_drop_cause::SoundDropCause;

/// The install's audio events as the effects file's sounds, each converted once as a weapon, an
/// effect or a look first names it (02-engine-core.md, An event is the game's, as data;
/// 09-zero-hour.md, A sound is PCM, the music the game's MP3): each file it names, its language's
/// first, read once and held, an IMA ADPCM WAV decoded to 16-bit PCM, a PCM WAV and a track's MP3
/// kept as they are; and what it drops of each, counted.
#[derive(Debug)]
pub(crate) struct SoundImport<'a, R> {
    audio: &'a AudioIni,
    language: Option<&'a str>,
    read: R,
    events: BTreeMap<DeclaredName, SoundEvent>,
    /// Each event the import converted or dropped, by the game's name: its package's name, none
    /// for one dropped.
    named: BTreeMap<String, Option<DeclaredName>>,
    /// The game's name of each package's name, which two of one name would share.
    taken: BTreeMap<DeclaredName, String>,
    /// Each file a sound plays, by where the package holds it, none for one that did not read.
    files: BTreeMap<PackagePath, Option<Vec<u8>>>,
    losses: Losses<SoundDropCause>,
}

/// What the import made of the sounds: each event by its package's name, each file's bytes by
/// its path, and what it dropped.
#[derive(Debug)]
pub(crate) struct ImportedSounds {
    pub(crate) events: BTreeMap<DeclaredName, SoundEvent>,
    pub(crate) files: BTreeMap<PackagePath, Vec<u8>>,
    pub(crate) dropped: Vec<Dropped<SoundDropCause>>,
}

/// The base sides whose skirmish music the scripts play, each by the name its `Game_` tracks
/// take (`SkirmishScripts.scb`).
const SIDE_TRACKS: [(&str, &str); 3] = [("USA", "USA"), ("China", "CHI"), ("GLA", "GLA")];

/// The track the skirmish scripts play between two of a side's.
const SILENCE: &str = "Game_Silent";

impl<'a, R: FnMut(&ArchivePath) -> Result<Option<Vec<u8>>, ZeroHourError>> SoundImport<'a, R> {
    /// An import of the events of `audio`, its files of the language `language` first, each
    /// file's bytes as `read` gives them, none for a file no archive holds.
    pub(crate) fn new(audio: &'a AudioIni, language: Option<&'a str>, read: R) -> Self {
        Self {
            audio,
            language,
            read,
            events: BTreeMap::new(),
            named: BTreeMap::new(),
            taken: BTreeMap::new(),
            files: BTreeMap::new(),
            losses: Losses::default(),
        }
    }

    /// The package's name of the event the game names `name`, converted as it is first named;
    /// none for an event no file defines, or of no file that reads, counted.
    pub(crate) fn event(&mut self, name: &str) -> Result<Option<DeclaredName>, ZeroHourError> {
        if let Some(converted) = self.named.get(name) {
            return Ok(converted.clone());
        }
        let converted = self.convert(name)?;
        self.named.insert(name.to_owned(), converted.clone());
        Ok(converted)
    }

    /// The voices of `voices`, each converted, the made unit's `VoiceCreate` before its
    /// `VoiceCreated`.
    pub(crate) fn voices(&mut self, voices: &IniVoices) -> Result<UnitVoices, ZeroHourError> {
        let mut voice = |name: &Option<String>| match name {
            Some(name) => self.event(name),
            None => Ok(None),
        };
        Ok(UnitVoices {
            select: voice(&voices.select)?,
            walk: voice(&voices.walk)?,
            attack: voice(&voices.attack)?,
            created: match voice(&voices.create)? {
                Some(created) => Some(created),
                None => voice(&voices.created)?,
            },
        })
    }

    /// The music of the choice `choice`: for each of `factions`, a value and its base side, the
    /// side's `Game_` tracks in order, each followed by `Game_Silent`, as the skirmish scripts
    /// cycle them; a faction of a side of no tracks plays none.
    pub(crate) fn music<'s>(
        &mut self,
        choice: DeclaredName,
        factions: impl IntoIterator<Item = (DeclaredName, &'s str)>,
    ) -> Result<Music, ZeroHourError> {
        let mut playlists = BTreeMap::new();
        for (faction, side) in factions {
            let Some(&(_, tracks)) = SIDE_TRACKS.iter().find(|(base, _)| *base == side) else {
                continue;
            };
            let mut playlist = Vec::new();
            for at in 1.. {
                let Some(track) = self.track(&format!("Game_{tracks}_{at:02}"))? else {
                    break;
                };
                playlist.push(track);
                playlist.extend(self.track(SILENCE)?);
            }
            if !playlist.is_empty() {
                playlists.insert(faction, playlist);
            }
        }
        Ok(Music { choice, playlists })
    }

    /// The sounds converted so far, by their package's names.
    pub(crate) const fn events(&self) -> &BTreeMap<DeclaredName, SoundEvent> {
        &self.events
    }

    /// Counts the event the game names `name` dropped for `cause`, as its use drops a part of
    /// how the game plays it.
    pub(crate) fn drop(&mut self, name: &str, cause: SoundDropCause) {
        self.losses.drop(name, cause);
    }

    /// What the import made, every file of a sound read.
    pub(crate) fn finish(self) -> ImportedSounds {
        let files = self.files.into_iter();
        ImportedSounds {
            events: self.events,
            files: files
                .filter_map(|(path, bytes)| Some((path, bytes?)))
                .collect(),
            dropped: self.losses.into_iter().collect(),
        }
    }

    /// The sound file of its settings and `music`: the positional sounds at their volume, the
    /// flat ones at theirs less their share past the positional ones, as the options apply
    /// `Relative2DVolume`, the music at its; the microphone; and the zoom's boost.
    pub(crate) fn sound_file(&self, music: Option<Music>) -> SoundFile {
        let settings = &self.audio.settings;
        let relative = settings.relative_2d_volume.clamp(-1.0, 1.0);
        let flat = if relative < 0.0 {
            settings.sound_volume * (1.0 + relative)
        } else {
            settings.sound_volume
        };
        let positional = if relative > 0.0 {
            settings.sound_3d_volume * (1.0 - relative)
        } else {
            settings.sound_3d_volume
        };
        let [near, far] = settings.zoom_range;
        SoundFile {
            volumes: SoundVolumes {
                positional,
                flat,
                music: settings.music_volume,
            },
            listener: Some(Listener {
                height: settings.microphone_height,
                share: settings.microphone_share,
            }),
            zoom: Some(ZoomBoost {
                near,
                far,
                boost: settings.zoom_share,
            }),
            music,
        }
    }

    /// The event `name` as a sound, its files read; none for one no file defines or of no file
    /// that reads, counted.
    fn convert(&mut self, name: &str) -> Result<Option<DeclaredName>, ZeroHourError> {
        let audio = self.audio;
        let Some(event) = audio.event(name) else {
            self.losses.drop(name, SoundDropCause::NoEvent);
            return Ok(None);
        };
        let declared = ImportName::latin(name).declared()?;
        if let Some(first) = self.taken.insert(declared.clone(), name.to_owned()) {
            return Err(NameError::Clash(NameClash {
                first,
                second: name.to_owned(),
            })
            .into());
        }
        let mut files = Vec::new();
        for stem in &event.sounds {
            files.extend(self.sound(name, stem)?);
        }
        if files.is_empty() {
            self.losses.drop(name, SoundDropCause::NoFile);
            return Ok(None);
        }
        let sound = Self::sound_event(event, files, &audio.settings, |cause| {
            self.losses.drop(name, cause);
        });
        self.events.insert(declared.clone(), sound);
        Ok(Some(declared))
    }

    /// The sound of `event` playing `files`, ranged by `settings` when it is `GLOBAL`, each part
    /// it drops given to `drop`.
    fn sound_event(
        event: &IniAudioEvent,
        files: Vec<PackagePath>,
        settings: &AudioSettings,
        mut drop: impl FnMut(SoundDropCause),
    ) -> SoundEvent {
        let types = event.types;
        for (dropped, cause) in [
            (event.control.looped, SoundDropCause::Loop),
            (event.attack_or_decay, SoundDropCause::AttackDecay),
            (event.time_of_day, SoundDropCause::TimeOfDay),
            (event.delay_ms != [0, 0], SoundDropCause::Delay),
            (event.low_pass > 0.0, SoundDropCause::LowPass),
            (types.shrouded, SoundDropCause::Shrouded),
        ] {
            if dropped {
                drop(cause);
            }
        }
        let [low, high] = event.pitch;
        let pitch = if low > 0.0 && low <= high && high.is_finite() {
            event.pitch
        } else {
            drop(SoundDropCause::Numbers);
            [1.0, 1.0]
        };
        let [full, silent] = if types.global {
            settings.global_range
        } else {
            [event.min_range, event.max_range]
        };
        let range = SoundRange { full, silent };
        let range = if !types.world {
            None
        } else if range.holds() {
            Some(range)
        } else {
            drop(SoundDropCause::Numbers);
            None
        };
        let count = event.limit.clamp(0, i32::from(u16::MAX));
        let count = u16::try_from(count).expect("a count held within u16");
        let limit = NonZeroU16::new(count).map(|count| SoundLimit {
            count,
            interrupt: event.control.interrupt,
        });
        let heard_by = if types.everyone {
            Hearers::Everyone
        } else if types.player {
            Hearers::Owner
        } else if types.allies {
            Hearers::Allies
        } else if types.enemies {
            Hearers::Enemies
        } else {
            Hearers::Everyone
        };
        SoundEvent {
            files,
            pick: if event.control.random {
                SoundPick::Random
            } else {
                SoundPick::InTurn
            },
            volume: event.volume.clamp(0.0, 1.0),
            loss: (-event.volume_shift).clamp(0.0, 1.0),
            pitch,
            limit,
            range,
            heard_by,
        }
    }

    /// The package's path of the sound of the stem `stem` of the event `event`, its file read
    /// and converted as it is first named; none for a file no archive holds or that does not
    /// decode, counted for the event.
    fn sound(&mut self, event: &str, stem: &str) -> Result<Option<PackagePath>, ZeroHourError> {
        let stem = stem.to_ascii_lowercase();
        let path = game_path::package_path(format!("client/sounds/{stem}.wav"))?;
        if let Some(read) = self.files.get(&path) {
            if read.is_none() {
                self.losses.drop(event, SoundDropCause::Encoding);
            }
            return Ok(read.as_ref().map(|_| path));
        }
        let localized = self.language.map(|language| {
            ArchivePath::of(&format!("data\\audio\\sounds\\{language}\\{stem}.wav"))
        });
        let general = ArchivePath::of(&format!("data\\audio\\sounds\\{stem}.wav"));
        let mut bytes = None;
        for from in localized.iter().chain([&general]) {
            bytes = (self.read)(from)?;
            if bytes.is_some() {
                break;
            }
        }
        let Some(bytes) = bytes else {
            self.losses.drop(event, SoundDropCause::NoFile);
            return Ok(None);
        };
        let pcm = match WavFile::read(&bytes) {
            // A PCM file whose chunks hold their sizes is copied as the game holds it; one cut
            // short is written again with its sizes exact, as every decoder reads them.
            Ok(file) if file.encoding == WavEncoding::Pcm16 && file.whole => Some(bytes),
            Ok(file) => Some(file.pcm_file(&file.decode())),
            Err(_) => None,
        };
        let read = pcm.is_some();
        self.files.insert(path.clone(), pcm);
        if !read {
            self.losses.drop(event, SoundDropCause::Encoding);
        }
        Ok(read.then_some(path))
    }

    /// The package's path of the track `name`, its MP3 read as it is first named; none for a
    /// track no file defines or no archive holds.
    fn track(&mut self, name: &str) -> Result<Option<PackagePath>, ZeroHourError> {
        let Some(track) = self.audio.track(name) else {
            return Ok(None);
        };
        let file = track.file.to_ascii_lowercase();
        let path = game_path::package_path(format!("client/music/{file}"))?;
        if self.files.contains_key(&path) {
            return Ok(Some(path));
        }
        let Some(bytes) = (self.read)(&ArchivePath::of(&format!("data\\audio\\tracks\\{file}")))?
        else {
            return Ok(None);
        };
        self.files.insert(path.clone(), Some(bytes));
        Ok(Some(path))
    }
}

#[cfg(test)]
mod tests;
