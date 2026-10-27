use std::cell::RefCell;

use super::*;
use crate::zero_hour::format::wav::samples::wav;

/// The events and tracks the tests convert: a rifle of three stems, a voice, a looped engine of
/// a global range, an event of no file, and America's two tracks and the silence.
const AUDIO: &str = "AudioEvent DefaultSoundEffect
  Volume = 100
  PitchShift = -0 0
  Control = random
  Limit = 4
  MinRange = 175
  MaxRange = 800
  Type = ui player
End
AudioEvent RangerWeapon
  Sounds = iranweaa iranweab iranlost
  Control = random interrupt
  PitchShift = -2 2
  VolumeShift = -15
  Limit = 3
  Volume = 60
  Type = world shrouded everyone
End
AudioEvent RangerVoiceSelect
  Sounds = iransea
  Control = random
  Type = ui voice player
End
AudioEvent Engine
  Sounds = vengine
  Control = loop
  Type = world global allies
End
AudioEvent Silent
  Sounds = gone
End
MusicTrack Game_USA_01
  Filename = USA_01.mp3
End
MusicTrack Game_USA_02
  Filename = USA_02.mp3
End
MusicTrack Game_Silent
  Filename = Silence60.mp3
End
";

/// The archives' files: the rifle's first stem in English and in the general folder, its second
/// as IMA ADPCM, the voice and the engine as PCM, and the tracks.
fn archives() -> BTreeMap<ArchivePath, Vec<u8>> {
    let pcm = wav([1, 1, 2, 16], 22_050, None, &[1, 0, 2, 0]);
    let adpcm = wav([17, 1, 6, 4], 22_050, None, &[100, 0, 0, 0, 0x07, 0x08]);
    [
        ("data\\audio\\sounds\\english\\iranweaa.wav", pcm.clone()),
        (
            "data\\audio\\sounds\\iranweaa.wav",
            b"not this one".to_vec(),
        ),
        ("data\\audio\\sounds\\iranweab.wav", adpcm),
        ("data\\audio\\sounds\\iransea.wav", pcm.clone()),
        ("data\\audio\\sounds\\vengine.wav", pcm),
        ("data\\audio\\tracks\\usa_01.mp3", b"ID3 one".to_vec()),
        ("data\\audio\\tracks\\usa_02.mp3", b"ID3 two".to_vec()),
        ("data\\audio\\tracks\\silence60.mp3", b"ID3 hush".to_vec()),
    ]
    .into_iter()
    .map(|(path, bytes)| (ArchivePath::of(path), bytes))
    .collect()
}

#[test]
fn an_event_converts_once_its_files_read_once_its_language_first_and_adpcm_decoded() {
    let mut audio = AudioIni::default();
    audio.read(AUDIO.as_bytes()).unwrap();
    let held = archives();
    let reads = RefCell::new(Vec::new());
    let mut sounds = SoundImport::new(&audio, Some("english"), |path: &ArchivePath| {
        reads.borrow_mut().push(path.as_str().to_owned());
        Ok(held.get(path).cloned())
    });
    let name = |text: &str| DeclaredName::new(text).unwrap();
    let path = |text: &str| PackagePath::parse(text).unwrap();
    // The rifle: its English first stem, its decoded second, its third of no file counted; its
    // volume of 60 % losing up to 15 %, 2 % lower to higher, 3 at once, the fourth stopping the
    // oldest, from 175 to 800, for everyone, and its shroud counted.
    assert_eq!(
        sounds.event("RangerWeapon").unwrap(),
        Some(name("rangerweapon"))
    );
    assert_eq!(
        sounds.event("RangerWeapon").unwrap(),
        Some(name("rangerweapon"))
    );
    let rifle = &sounds.events()[&name("rangerweapon")];
    assert_eq!(
        *rifle,
        SoundEvent {
            files: vec![
                path("client/sounds/iranweaa.wav"),
                path("client/sounds/iranweab.wav"),
            ],
            pick: SoundPick::Random,
            volume: 0.6,
            loss: 0.15,
            pitch: [1.0 + -2.0 / 100.0, 1.0 + 2.0 / 100.0],
            limit: Some(SoundLimit {
                count: NonZeroU16::new(3).unwrap(),
                interrupt: true,
            }),
            range: Some(SoundRange {
                full: 175.0,
                silent: 800.0,
            }),
            heard_by: Hearers::Everyone,
        }
    );
    // The voice flat, for its owner; the engine of the global range, for its allies, its loop
    // counted; no event, and an event of no file, none.
    assert_eq!(
        sounds.event("RangerVoiceSelect").unwrap(),
        Some(name("rangervoiceselect"))
    );
    let voice = &sounds.events()[&name("rangervoiceselect")];
    assert_eq!((voice.range, voice.heard_by), (None, Hearers::Owner));
    // No settings read, the engine's global range of 0 to 0 does not hold: it plays flat,
    // counted.
    assert_eq!(sounds.event("Engine").unwrap(), Some(name("engine")));
    let engine = &sounds.events()[&name("engine")];
    assert_eq!((engine.range, engine.heard_by), (None, Hearers::Allies));
    assert_eq!(sounds.event("Nothing").unwrap(), None);
    assert_eq!(sounds.event("Silent").unwrap(), None);
    // Each file read once, its language's tried first: the rifle's first found in English, its
    // second and the voice's and the engine's in the general folder, the lost and the gone in
    // neither, the second time the rifle is named none.
    let english = |stem: &str| format!("data\\audio\\sounds\\english\\{stem}.wav");
    let general = |stem: &str| format!("data\\audio\\sounds\\{stem}.wav");
    assert_eq!(
        *reads.borrow(),
        [
            english("iranweaa"),
            english("iranweab"),
            general("iranweab"),
            english("iranlost"),
            general("iranlost"),
            english("iransea"),
            general("iransea"),
            english("vengine"),
            general("vengine"),
            english("gone"),
            general("gone"),
        ]
    );

    // The music: America's tracks in order, the silence after each; a side of no tracks plays
    // none, and a faction of no known side is left out.
    let music = sounds
        .music(
            name("faction"),
            [
                (name("america"), "USA"),
                (name("china"), "China"),
                (name("civilian"), "Civilian"),
            ],
        )
        .unwrap();
    let usa = [
        "client/music/usa_01.mp3",
        "client/music/silence60.mp3",
        "client/music/usa_02.mp3",
        "client/music/silence60.mp3",
    ];
    assert_eq!(
        music,
        Music {
            choice: name("faction"),
            playlists: BTreeMap::from([(name("america"), usa.map(path).to_vec())]),
        }
    );
    let imported = sounds.finish();
    // The files kept: the rifle's English first stem as it is, its ADPCM second as PCM of its
    // five samples, the voice, the engine and the three tracks.
    assert_eq!(imported.files.len(), 7);
    let decoded = WavFile::read(&imported.files[&path("client/sounds/iranweab.wav")]).unwrap();
    assert_eq!(decoded.encoding, WavEncoding::Pcm16);
    assert_eq!(
        imported.files[&path("client/sounds/iranweaa.wav")],
        archives()[&ArchivePath::of("data\\audio\\sounds\\english\\iranweaa.wav")]
    );
    assert_eq!(imported.files[&path("client/music/usa_02.mp3")], b"ID3 two");
    let dropped = |name: &str, cause| Dropped {
        name: name.to_owned(),
        cause,
    };
    assert_eq!(
        imported.dropped,
        [
            dropped("RangerWeapon", SoundDropCause::NoFile),
            dropped("RangerWeapon", SoundDropCause::Shrouded),
            dropped("Engine", SoundDropCause::Loop),
            dropped("Engine", SoundDropCause::Numbers),
            dropped("Nothing", SoundDropCause::NoEvent),
            dropped("Silent", SoundDropCause::NoFile),
        ]
    );
}

#[test]
fn voices_take_the_made_units_create_first_and_the_settings_make_the_sound_file() {
    let mut audio = AudioIni::default();
    audio.read(AUDIO.as_bytes()).unwrap();
    audio
        .read_settings(
            b"AudioSettings\n  GlobalMinRange = 5000\n  GlobalMaxRange = 500000\n  \
              Relative2DVolume = -10%\n  DefaultSoundVolume = 80%\n  Default3DSoundVolume = 80%\n  \
              DefaultMusicVolume = 55%\n  MicrophoneDesiredHeightAboveTerrain = 50.0\n  \
              MicrophoneMaxPercentageBetweenGroundAndCamera = 33.3%\n  ZoomMinDistance = 130.0\n  \
              ZoomMaxDistance = 425.0\n  ZoomSoundVolumePercentageAmount = 20%\nEnd\n",
        )
        .unwrap();
    let held = archives();
    let mut sounds = SoundImport::new(&audio, None, |path: &ArchivePath| {
        Ok(held.get(path).cloned())
    });
    let name = |text: &str| DeclaredName::new(text).unwrap();
    // The ranger's select voice and its `VoiceCreate`, which comes before its `VoiceCreated`; a
    // voice of no event is none.
    let voices = IniVoices {
        select: Some("RangerVoiceSelect".to_owned()),
        walk: Some("Nothing".to_owned()),
        attack: None,
        created: Some("Engine".to_owned()),
        create: Some("RangerWeapon".to_owned()),
    };
    assert_eq!(
        sounds.voices(&voices).unwrap(),
        UnitVoices {
            select: Some(name("rangervoiceselect")),
            walk: None,
            attack: None,
            created: Some(name("rangerweapon")),
        }
    );
    // The engine of the global range: 5,000 to 500,000.
    assert_eq!(sounds.event("Engine").unwrap(), Some(name("engine")));
    assert_eq!(
        sounds.events()[&name("engine")].range,
        Some(SoundRange {
            full: 5000.0,
            silent: 500_000.0,
        })
    );
    // Positional sounds at 80 %, flat ones 10 % under, 0.8 × 0.9, the music at 55 %; the
    // microphone 50 m up or a third of the way; the zoom's boost of 20 % from 425 m to 130.
    assert_eq!(
        sounds.sound_file(None),
        SoundFile {
            volumes: SoundVolumes {
                positional: 0.8,
                flat: 0.8 * (1.0 + -0.1),
                music: 0.55,
            },
            listener: Some(Listener {
                height: 50.0,
                share: 33.3 / 100.0,
            }),
            zoom: Some(ZoomBoost {
                near: 130.0,
                far: 425.0,
                boost: 0.2,
            }),
            music: None,
        }
    );
}
