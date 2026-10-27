use super::*;

/// The default sound effect, a rifle's shot of its fields, a voice of the default's but its
/// files and type, a loop of an attack and a night's sounds, and the default track and a track.
const AUDIO: &str = "AudioEvent DefaultSoundEffect
  Volume = 100
  MinVolume = 40
  Control = random
  Limit = 4
  MinRange = 175.00
  MaxRange = 800.00
  Type = ui player
End
AudioEvent RangerWeapon
  Sounds = iranweaa iranweab,iranweac ;iranweaf
  Control= random interrupt
  PitchShift = -2 2
  VolumeShift= -15
  Limit = 3
  Volume = 60
  Delay = 10 20
  Type = world shrouded everyone
End
AudioEvent RangerVoiceSelect
  Sounds = iransea iranseb
  Type = ui voice player
End
AudioEvent Engine
  Sounds = vengine
  Attack = vengstart
  SoundsNight = vengnight
  LowPassCutoff = 50
  Control = loop
  LoopCount = 0
End
MusicTrack DefaultMusicTrack
  Filename = NoFilename
  Volume = 100.0
End
MusicTrack Game_USA_01
  Filename = USA_01.mp3
End
DialogEvent Speech
  Filename = speech.wav
End
";

#[test]
fn an_event_starts_from_the_default_and_takes_its_own_fields() {
    let mut ini = AudioIni::default();
    ini.read(AUDIO.as_bytes()).unwrap();
    let stems = |names: &[&str]| {
        names
            .iter()
            .map(|&name| name.to_owned())
            .collect::<Vec<_>>()
    };
    let default = IniAudioEvent {
        volume: 1.0,
        min_volume: 0.4,
        limit: 4,
        min_range: 175.0,
        max_range: 800.0,
        types: SoundTypes {
            ui: true,
            player: true,
            ..SoundTypes::default()
        },
        control: SoundControl {
            random: true,
            ..SoundControl::default()
        },
        ..IniAudioEvent::default()
    };
    // The rifle's stems cut at a comma, its comment dropped; its fields replace the default's,
    // its types and controls whole.
    assert_eq!(
        ini.event("RangerWeapon"),
        Some(&IniAudioEvent {
            sounds: stems(&["iranweaa", "iranweab", "iranweac"]),
            volume: 0.6,
            volume_shift: -0.15,
            // 1 − 2 / 100 and 1 + 2 / 100, each rounded once more than the percent alone.
            pitch: [1.0 + -2.0 / 100.0, 1.0 + 2.0 / 100.0],
            delay_ms: [10, 20],
            limit: 3,
            types: SoundTypes {
                world: true,
                shrouded: true,
                everyone: true,
                ..SoundTypes::default()
            },
            control: SoundControl {
                random: true,
                interrupt: true,
                ..SoundControl::default()
            },
            ..default
        })
    );
    assert_eq!(
        ini.event("RangerVoiceSelect"),
        Some(&IniAudioEvent {
            sounds: stems(&["iransea", "iranseb"]),
            types: SoundTypes {
                ui: true,
                voice: true,
                player: true,
                ..SoundTypes::default()
            },
            ..default
        })
    );
    let engine = ini.event("Engine").unwrap();
    assert!(engine.control.looped && engine.attack_or_decay && engine.time_of_day);
    assert_eq!(engine.low_pass, 0.5);
    // A name matches its case alone, as the game's hash finds it.
    assert_eq!(ini.event("rangerweapon"), None);
    // A track takes the default track's volume.
    assert_eq!(
        ini.track("Game_USA_01"),
        Some(&IniTrack {
            file: "USA_01.mp3".to_owned(),
            volume: 1.0,
        })
    );

    // An event read again starts from the default, not from its first definition.
    ini.read(b"AudioEvent RangerWeapon\n  Sounds = iranweaf\nEnd\n")
        .unwrap();
    assert_eq!(
        ini.event("RangerWeapon"),
        Some(&IniAudioEvent {
            sounds: stems(&["iranweaf"]),
            ..default
        })
    );
    // A type it does not list is refused.
    assert_eq!(
        AudioIni::default()
            .read(b"AudioEvent A\n  Type = loud\nEnd\n")
            .unwrap_err(),
        IniError::NotListed { line: 2 }
    );
}

#[test]
fn the_settings_give_the_ranges_volumes_microphone_and_zoom() {
    let mut ini = AudioIni::default();
    ini.read_settings(
        b"AudioSettings
  GlobalMinRange = 5000
  GlobalMaxRange = 500000
  Relative2DVolume = -10%
  DefaultSoundVolume = 80%
  Default3DSoundVolume = 80%
  DefaultMusicVolume = 55%
  MicrophoneDesiredHeightAboveTerrain = 50.0
  MicrophoneMaxPercentageBetweenGroundAndCamera = 33.3%
  ZoomMinDistance = 130.0
  ZoomMaxDistance = 425.0
  ZoomSoundVolumePercentageAmount = 20%
  SampleCount3D = 25
End
",
    )
    .unwrap();
    assert_eq!(
        ini.settings,
        AudioSettings {
            global_range: [5000.0, 500_000.0],
            sound_volume: 0.8,
            sound_3d_volume: 0.8,
            music_volume: 0.55,
            relative_2d_volume: -0.1,
            microphone_height: 50.0,
            microphone_share: 33.3 / 100.0,
            zoom_range: [130.0, 425.0],
            zoom_share: 0.2,
        }
    );
}
