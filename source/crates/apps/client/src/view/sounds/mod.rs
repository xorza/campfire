use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::app::{App, Plugin, PostStartup, Update};
use bevy::asset::{AssetPath, AssetServer, Handle};
use bevy::audio::{
    AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, PlaybackSettings, SpatialAudioSink,
    SpatialListener, Volume,
};
use bevy::camera::Camera3d;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::message::MessageReader;
use bevy::ecs::query::{Added, Allow, Has, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::common_conditions::resource_exists;
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut, Single};
use bevy::math::Vec3;
use bevy::transform::components::{GlobalTransform, Transform};
use campfire_client_files::{
    Hearers, Listener, Music, SoundFile, SoundLimit, SoundPick, ZoomBoost,
};
use campfire_common::{DeclaredName, PlayerSlot};
use campfire_host::JoinState;
use campfire_math::SplitMix64;
use campfire_mode::{ChoiceData, PlayerSlotPicks};
use campfire_package::ModePackages;
use campfire_production::MadeAt;
use campfire_sim::{EntityIndex, Unpredicted};
use campfire_units::{Leaving, Owner, TypeOriginBook};
use campfire_values::{NameId, PackageIndex, PackagePath, Relation, Team, UnitType};
use tracing::error;

use crate::own_player::OwnPlayer;
use crate::selection::{Selection, SelectionSystems, SelectionVoice};
use crate::view::ViewSystems;
use crate::view::client_data::ClientLoad;
use crate::view::effects::MatchEffects;
use crate::view::effects::effect_book::{EffectBook, SoundId};
use crate::view::effects::particle_run::{HeardAt, SoundSource};
use crate::view::ground_heights::GroundHeights;
use crate::view::package_source::PackageAssets;

/// The match's sounds (02-engine-core.md, The client plays by Bevy's audio; 02-engine-core.md, A
/// sound falls off as the release game's and fades to its range's end; 02-engine-core.md, The
/// listener is the game's microphone; 02-engine-core.md, A look names its voices;
/// 02-engine-core.md, Music by a choice): each sound an effect, a fire or a voice plays starts once
/// the frame's effects ran, if its player hears it and its limit leaves room, of a file its pick
/// draws, at a volume and a pitch drawn of its ranges; a positional one sounds from its place, its
/// volume following the listener each frame by its fade and the zoom; a flat one sounds at its
/// volume; and the music plays the playlist of the player's pick, one track after another, from its
/// first again after its last.
#[derive(Debug)]
pub(crate) struct SoundsPlugin {
    pub(crate) packages: Arc<ModePackages>,
}

/// The match's sounds: each sound's files, by the sound's id, the turn of each sound of files in
/// turn and the last file each drew, the mode's sound file, each type's voices, the playlist of the
/// player's pick, once known, and its next track, the plays started, which order them by age, and
/// the dice of the draws.
#[derive(Resource, Debug)]
struct MatchSounds {
    files: Vec<Box<[Handle<AudioSource>]>>,
    picks: Vec<FilePick>,
    mix: SoundFile,
    voices: Vec<TypeVoices>,
    playlist: Option<Box<[Handle<AudioSource>]>>,
    track: usize,
    started: u64,
    dice: SplitMix64,
}

/// The file a sound's play draws next: the last drawn, none before the first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FilePick {
    last: Option<usize>,
}

/// A type's voices, each by its sound's id.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct TypeVoices {
    select: Option<SoundId>,
    walk: Option<SoundId>,
    attack: Option<SoundId>,
    created: Option<SoundId>,
}

/// A sound playing: its id, its place, none for a flat one, the volume its draw gave, and the
/// count of plays before it, its age.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
struct PlayingSound {
    sound: SoundId,
    at: Option<Vec3>,
    volume: f32,
    started: u64,
}

/// A play alive this frame, of the plays before it and those it started: its entity, its sound
/// and its age.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LivePlay {
    entity: Entity,
    sound: SoundId,
    started: u64,
}

/// What a sound's limit leaves a new play of it: room, none, or the place of the oldest play it
/// stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Room {
    Free,
    Full,
    Oldest(usize),
}

/// The units new to the client: each one's type, place, player and team, whether it leaves its
/// producer, and whether a producer made it at a place, which say a producer made it.
type TrainedUnits<'w, 's> = Query<
    'w,
    's,
    (
        &'static UnitType,
        &'static GlobalTransform,
        &'static Owner,
        &'static Team,
        Option<&'static Leaving>,
        Has<MadeAt>,
    ),
    (Added<UnitType>, Allow<Unpredicted>),
>;

/// A track of the music playing.
#[derive(Component, Debug)]
struct MusicTrack;

/// The listener's ears, which stand at the microphone and turn with the camera.
#[derive(Component, Debug)]
struct Ears;

/// Where the listener hears from this frame, and the share of the positional volume the zoom
/// gives.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
struct Hearing {
    at: Vec3,
    zoom: f32,
}

/// The gap between the listener's ears, and the distance from it each positional play sounds
/// from, in meters: near enough that rodio's fall-off by the inverse square, `min(1, 1 / d²)`,
/// holds each ear at its whole, so the play's own fade alone sets its volume, and the gap a
/// third of it, so the direction pans it.
const EAR_GAP: f32 = 0.2;
const EMITTER: f32 = 0.5;

impl Plugin for SoundsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Hearing {
            at: Vec3::ZERO,
            zoom: 1.0,
        });
        app.insert_resource(PackagesHeld(Arc::clone(&self.packages)));
        app.add_systems(PostStartup, Self::load.before(ClientLoad::drop));
        app.add_systems(
            Update,
            (
                Self::listen,
                (Self::voice, Self::play, Self::mix, Self::music)
                    .chain()
                    .run_if(resource_exists::<MatchSounds>),
            )
                .chain()
                .after(ViewSystems)
                .after(SelectionSystems),
        );
    }
}

impl SoundsPlugin {
    /// Loads each sound's files and each type's voices, of the effects the effects' load read,
    /// none in a mode of no effects, which still plays its music; logs a choice or a value of the
    /// music the mode lacks, which plays no music.
    fn load(
        (load, packages): (Res<'_, ClientLoad>, Res<'_, PackagesHeld>),
        effects: Option<Res<'_, MatchEffects>>,
        origins: Res<'_, TypeOriginBook>,
        assets: Res<'_, AssetServer>,
        mut commands: Commands<'_, '_>,
    ) {
        let book = effects.as_deref().map(|effects| &effects.book);
        let path = |path| mode_audio(&assets, path);
        let sounds = book.map_or(&[][..], EffectBook::sounds);
        let files = sounds
            .iter()
            .map(|sound| sound.files.iter().map(path).collect())
            .collect();
        let voices = origins
            .iter()
            .map(|(_, origin)| {
                let look = load.of(origin.package).units.units().get(&*origin.name);
                let voices = look.map(|look| &look.voices);
                let id = |name: Option<&DeclaredName>| {
                    let name = name?;
                    let id = book.and_then(|book| book.sound_id(name));
                    if id.is_none() {
                        let (look, voice) = (&*origin.name, name.as_str());
                        error!(
                            look,
                            voice, "a look's voice names no sound of the effects file"
                        );
                    }
                    id
                };
                voices.map_or_else(TypeVoices::default, |voices| TypeVoices {
                    select: id(voices.select.as_ref()),
                    walk: id(voices.walk.as_ref()),
                    attack: id(voices.attack.as_ref()),
                    created: id(voices.created.as_ref()),
                })
            })
            .collect();
        if let Some(music) = &load.sound.music {
            let choice = music.choice.as_str();
            match packages.0.data().choices.get(&music.choice) {
                None => error!(choice, "the music names no choice of the mode"),
                Some(data) => {
                    let unknown = music
                        .playlists
                        .keys()
                        .filter(|value| !data.values.contains(value));
                    for value in unknown {
                        let value = value.as_str();
                        error!(choice, value, "the music names no value of its choice");
                    }
                }
            }
        }
        commands.spawn((Ears, SpatialListener::new(EAR_GAP), Transform::default()));
        commands.insert_resource(MatchSounds {
            files,
            picks: vec![FilePick::default(); sounds.len()],
            mix: load.sound.clone(),
            voices,
            playlist: None,
            track: 0,
            started: 0,
            dice: SplitMix64::new(0x5eed_50da),
        });
    }

    /// Stands the listener at the microphone of the camera, facing as it faces, and takes the
    /// zoom's share of the positional volume.
    fn listen(
        camera: Single<'_, '_, &GlobalTransform, (With<Camera3d>, Without<Ears>)>,
        heights: Option<Res<'_, GroundHeights>>,
        sounds: Option<Res<'_, MatchSounds>>,
        mut ears: Query<'_, '_, &mut Transform, With<Ears>>,
        mut hearing: ResMut<'_, Hearing>,
    ) {
        let (_, turn, camera) = camera.to_scale_rotation_translation();
        let forward = turn * Vec3::NEG_Z;
        let ground = match heights.as_deref() {
            Some(heights) => heights.hit(camera, forward),
            None => ground_plane(camera, forward),
        };
        let mix = sounds.as_deref().map(|sounds| &sounds.mix);
        let listener = mix.and_then(|mix| mix.listener);
        let at = ground.map_or(camera, |ground| microphone(ground, camera, listener));
        let zoom = mix
            .and_then(|mix| mix.zoom)
            .map_or(1.0, |zoom| zoom_volume(camera.distance(at), zoom));
        *hearing = Hearing { at, zoom };
        for mut transform in &mut ears {
            *transform = Transform::from_translation(at).with_rotation(turn);
        }
    }

    /// Asks each voice of the frame: each voice the player's pick and orders ask of the
    /// selection, of its first unit of one, and the created voice of each unit of the player's
    /// that a producer made; none in a mode of no effects, whose looks name no sound.
    fn voice(
        own: OwnPlayer<'_, '_>,
        (selection, index): (Res<'_, Selection>, Res<'_, EntityIndex>),
        units: Query<'_, '_, (&UnitType, &GlobalTransform, &Team), Allow<Unpredicted>>,
        made: TrainedUnits<'_, '_>,
        mut asked: MessageReader<'_, '_, SelectionVoice>,
        effects: Option<ResMut<'_, MatchEffects>>,
        sounds: Res<'_, MatchSounds>,
    ) {
        let Some(mut effects) = effects else {
            return;
        };
        let own_slot = own.player_slot();
        let mut ask = |voice: SelectionVoice| {
            let first = selection.get().iter().find_map(|&unit| {
                let (unit_type, place, &team) = units.get(index.get(unit)?).ok()?;
                let voices = sounds.voices.get(unit_type.index())?;
                let sound = voices.of(voice)?;
                Some((sound, place.translation(), team))
            });
            if let Some((sound, at, team)) = first {
                effects.run.hear(HeardAt {
                    sound,
                    at,
                    source: Some(SoundSource {
                        owner: own_slot,
                        team,
                    }),
                });
            }
        };
        for &voice in asked.read() {
            ask(voice);
        }
        for (unit_type, place, owner, &team, leaving, made_at) in &made {
            let trained = leaving.is_some_and(|leaving| leaving.get()) || made_at;
            if !trained || Some(owner.player_slot()) != own_slot {
                continue;
            }
            let voices = sounds.voices.get(unit_type.index());
            if let Some(sound) = voices.and_then(|voices| voices.created) {
                effects.run.hear(HeardAt {
                    sound,
                    at: place.translation(),
                    source: Some(SoundSource {
                        owner: own_slot,
                        team,
                    }),
                });
            }
        }
    }

    /// Starts each sound the frame's effects, fires and voices played that the player hears and
    /// its limit leaves room for among the plays alive and those the frame started, the oldest of
    /// an interrupting sound stopped in its place.
    fn play(
        own: OwnPlayer<'_, '_>,
        effects: Option<ResMut<'_, MatchEffects>>,
        mut sounds: ResMut<'_, MatchSounds>,
        playing: Query<'_, '_, (Entity, &PlayingSound)>,
        mut commands: Commands<'_, '_>,
        mut live: Local<'_, Vec<LivePlay>>,
    ) {
        let Some(mut effects) = effects else {
            return;
        };
        let MatchEffects { book, run, .. } = &mut *effects;
        live.clear();
        live.extend(playing.iter().map(|(entity, play)| LivePlay {
            entity,
            sound: play.sound,
            started: play.started,
        }));
        let relation = |team| own.relation_to(team);
        for heard in run.take_heard() {
            let event = book.sound(heard.sound);
            if !hears(event.heard_by, heard.source, own.player_slot(), relation) {
                continue;
            }
            let same = live
                .iter()
                .enumerate()
                .filter(|(_, play)| play.sound == heard.sound)
                .map(|(at, play)| (at, play.started));
            match room(event.limit, same) {
                Room::Free => {}
                Room::Full => continue,
                Room::Oldest(at) => {
                    commands.entity(live.swap_remove(at).entity).despawn();
                }
            }
            let sounds = &mut *sounds;
            let files = &sounds.files[heard.sound.index()];
            let pick = &mut sounds.picks[heard.sound.index()];
            let count = u64::try_from(files.len()).expect("a count fits u64");
            let drawn = usize::try_from(sounds.dice.below(count)).expect("below a count of files");
            let at = pick.next(event.pick, files.len(), drawn);
            let loss = share(&mut sounds.dice);
            let volume = event.volume * (1.0 - event.loss * loss);
            let rate = share(&mut sounds.dice);
            let [low, high] = event.pitch;
            let speed = low + (high - low) * rate;
            let positional = event.range.is_some();
            sounds.started += 1;
            let entity = commands.spawn((
                AudioPlayer(files[at].clone()),
                PlaybackSettings {
                    volume: Volume::Linear(0.0),
                    speed,
                    spatial: positional,
                    ..PlaybackSettings::DESPAWN
                },
                Transform::default(),
                PlayingSound {
                    sound: heard.sound,
                    at: positional.then_some(heard.at),
                    volume,
                    started: sounds.started,
                },
            ));
            live.push(LivePlay {
                entity: entity.id(),
                sound: heard.sound,
                started: sounds.started,
            });
        }
    }

    /// Sets each play's volume by its draw and its class, a positional one's by its fade from the
    /// listener and the zoom too, and stands a positional one's emitter in its direction from the
    /// listener.
    fn mix(
        (effects, sounds, hearing): (
            Option<Res<'_, MatchEffects>>,
            Res<'_, MatchSounds>,
            Res<'_, Hearing>,
        ),
        mut flat: Query<'_, '_, (&PlayingSound, &mut AudioSink)>,
        mut positional: Query<'_, '_, (&PlayingSound, &mut SpatialAudioSink, &mut Transform)>,
    ) {
        let Some(effects) = effects else {
            return;
        };
        let volumes = sounds.mix.volumes;
        for (play, mut sink) in &mut flat {
            sink.set_volume(Volume::Linear(play.volume * volumes.flat));
        }
        for (play, mut sink, mut transform) in &mut positional {
            let (Some(at), Some(range)) = (play.at, effects.book.sound(play.sound).range) else {
                continue;
            };
            let away = at - hearing.at;
            let fade = fade(away.length(), range.full, range.silent);
            let volume = play.volume * volumes.positional * hearing.zoom * fade;
            sink.set_volume(Volume::Linear(volume));
            transform.translation = hearing.at + away.normalize_or_zero() * EMITTER;
        }
    }

    /// Plays the music: the playlist of the player's pick of the music's choice, taken once the
    /// session's terms give the picks, none for a pick of no playlist; each track after the last
    /// ends, from its first again after its last.
    fn music(
        (own, join, packages): (
            OwnPlayer<'_, '_>,
            Option<Res<'_, JoinState>>,
            Res<'_, PackagesHeld>,
        ),
        mut sounds: ResMut<'_, MatchSounds>,
        playing: Query<'_, '_, (), With<MusicTrack>>,
        assets: Res<'_, AssetServer>,
        mut commands: Commands<'_, '_>,
    ) {
        if sounds.playlist.is_none() {
            let picks = join
                .as_deref()
                .map(JoinState::player_slot_picks)
                .filter(|picks| !picks.is_empty());
            let (Some(music), Some(picks), Some(player_slot)) =
                (sounds.mix.music.as_ref(), picks, own.player_slot())
            else {
                return;
            };
            let choices = &packages.0.data().choices;
            let tracks = playlist(music, choices, picks, player_slot).unwrap_or_default();
            let load = |path| mode_audio(&assets, path);
            sounds.playlist = Some(tracks.iter().map(load).collect());
        }
        if !playing.is_empty() {
            return;
        }
        let sounds = &mut *sounds;
        let Some(playlist) = sounds.playlist.as_deref() else {
            return;
        };
        if playlist.is_empty() {
            return;
        }
        let track = playlist[sounds.track % playlist.len()].clone();
        sounds.track = (sounds.track + 1) % playlist.len();
        commands.spawn((
            AudioPlayer(track),
            PlaybackSettings {
                volume: Volume::Linear(sounds.mix.volumes.music),
                ..PlaybackSettings::DESPAWN
            },
            MusicTrack,
        ));
    }
}

/// The mode's packages, whose choices the music's pick names.
#[derive(Resource, Debug)]
struct PackagesHeld(Arc<ModePackages>);

impl TypeVoices {
    /// Its sound of `voice`.
    const fn of(self, voice: SelectionVoice) -> Option<SoundId> {
        match voice {
            SelectionVoice::Select => self.select,
            SelectionVoice::Move => self.walk,
            SelectionVoice::Attack => self.attack,
        }
    }
}

impl FilePick {
    /// The file of `count` a play draws, `drawn` a place below `count` at random: of a random
    /// pick, the drawn one, the next one in place of the last drawn of a list of more than two, as
    /// `AudioEventRTS::generateFilename` draws it; of a pick in turn, the one after the last.
    fn next(&mut self, pick: SoundPick, count: usize, drawn: usize) -> usize {
        debug_assert!(drawn < count, "a place among the files");
        let at = match (pick, self.last) {
            (SoundPick::InTurn, Some(last)) => (last + 1) % count,
            (SoundPick::InTurn, None) => 0,
            (SoundPick::Random, last) if last == Some(drawn) && count > 2 => (drawn + 1) % count,
            (SoundPick::Random, _) => drawn,
        };
        self.last = Some(at);
        at
    }
}

/// The handle of the mode package's audio file at `path`, its load started.
fn mode_audio(assets: &AssetServer, path: &PackagePath) -> Handle<AudioSource> {
    assets.load(AssetPath::from(PackageAssets::asset_path(
        PackageIndex::MODE,
        path,
    )))
}

/// The room `limit` leaves a new play of a sound, `same` the place and the age of each play of it
/// alive, as `MilesAudioManager::doesViolateLimit` counts them: full at `limit`'s count, its
/// oldest stopped when it interrupts; free of no limit.
fn room(limit: Option<SoundLimit>, same: impl Iterator<Item = (usize, u64)> + Clone) -> Room {
    let Some(limit) = limit else {
        return Room::Free;
    };
    if same.clone().count() < usize::from(limit.count.get()) {
        Room::Free
    } else if limit.interrupt {
        let (oldest, _) = same
            .min_by_key(|&(_, started)| started)
            .expect("a full limit counts a play");
        Room::Oldest(oldest)
    } else {
        Room::Full
    }
}

/// A share from 0 to 1 of 24 bits of a draw of `dice`.
#[expect(clippy::cast_precision_loss, reason = "24 bits fit an f32 whole")]
fn share(dice: &mut SplitMix64) -> f32 {
    (dice.next_u64() >> 40) as f32 / (1_u32 << 24) as f32
}

/// Whether the player of `own` hears a sound `heard_by` its source `source`, by the player's
/// relation to a team as `relation` gives it: a sound of no source every player hears.
fn hears(
    heard_by: Hearers,
    source: Option<SoundSource>,
    own: Option<PlayerSlot>,
    relation: impl Fn(Team) -> Option<Relation>,
) -> bool {
    let Some(source) = source else {
        return true;
    };
    let mine = own.is_some() && source.owner == own;
    match heard_by {
        Hearers::Everyone => true,
        Hearers::Owner => mine,
        Hearers::Allies => !mine && relation(source.team) == Some(Relation::Friendly),
        Hearers::Enemies => relation(source.team) == Some(Relation::Hostile),
    }
}

/// The share of a positional sound's volume at `distance` from the listener: all of it up to
/// `full`; past it `full / distance`, as the release game falls off, times `1 − ((distance − full)
/// / (silent − full))⁴`, TheSuperHackers' fade in place of its cut; and none from `silent`
/// (02-engine-core.md, A sound falls off as the release game's and fades to its range's end).
fn fade(distance: f32, full: f32, silent: f32) -> f32 {
    if distance >= silent {
        0.0
    } else if distance <= full {
        1.0
    } else {
        full / distance * (1.0 - ((distance - full) / (silent - full)).powi(4))
    }
}

/// The microphone between `ground`, the camera's point on the ground, and `camera`: toward the
/// camera, `listener`'s height above the ground or its share of the way, whichever is nearer,
/// as `AudioManager::update` places it; the camera's own place for a mode of no listener.
fn microphone(ground: Vec3, camera: Vec3, listener: Option<Listener>) -> Vec3 {
    let Some(listener) = listener else {
        return camera;
    };
    let up = camera - ground;
    let share = if up.y <= listener.height || up.y <= 0.0 {
        listener.share
    } else {
        (listener.height / up.y).min(listener.share)
    };
    ground + up * share
}

/// The share of the positional volume a camera `distance` from the microphone gives: all of it
/// within `near`, `1 − boost` from `far`, and between them, the boost falling linearly.
fn zoom_volume(distance: f32, zoom: ZoomBoost) -> f32 {
    if distance < zoom.near {
        1.0
    } else if distance < zoom.far {
        1.0 - (distance - zoom.near) / (zoom.far - zoom.near) * zoom.boost
    } else {
        1.0 - zoom.boost
    }
}

/// Where a ray from `origin` along `direction` meets the plane at height 0; none for one that
/// does not.
fn ground_plane(origin: Vec3, direction: Vec3) -> Option<Vec3> {
    if direction.y >= 0.0 {
        return None;
    }
    Some(origin + direction * (-origin.y / direction.y))
}

/// The tracks of `music`'s playlist of the value the player of `player_slot` picked of its choice,
/// of the mode's `choices` and the terms' `picks`; none for no pick, or a value of no playlist.
fn playlist<'m>(
    music: &'m Music,
    choices: &BTreeMap<DeclaredName, ChoiceData>,
    picks: &[u8],
    player_slot: PlayerSlot,
) -> Option<&'m [PackagePath]> {
    let picks = PlayerSlotPicks::of_places(choices, picks).ok()?;
    let value = picks.value(choices, player_slot, &music.choice)?;
    music.playlists.get(value).map(Vec::as_slice)
}

#[cfg(test)]
mod tests;
