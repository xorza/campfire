# Zero Hour's audio

How Zero Hour plays its sounds, voices and music, how OpenRA does the same, and what the engine holds of it, read on 27 October 2026 from [GeneralsX](https://github.com/fbraz3/GeneralsX) (`AudioEventInfo.h`, `INIAudioEventInfo.cpp`, `AudioEventRTS.cpp`, `GameAudio.cpp`, `MilesAudioManager.cpp`, `CommandXlat.cpp`, `ThingTemplate.cpp`, `ProductionUpdate.cpp`), the install's `INIZH.big` (`AudioSettings.ini`, `SoundEffects.ini`, `Voice.ini`, `Speech.ini`, `Music.ini`, `MiscAudio.ini`, `FXList.ini`, `Weapon.ini`, `Default\`), its audio archives and `Data\Scripts\SkirmishScripts.scb`, rodio 0.22's `spatial.rs` and bevy_audio 0.20's features, and the engine's code.

## The files

- **3,530 sound files in the Zero Hour archives, 5,108 in the base game's.** `AudioZH.big`, `AudioEnglishZH.big`, `SpeechZH.big`, `SpeechEnglishZH.big` and `MusicZH.big`, and the base game's `Audio.big`, `AudioEnglish.big`, `Speech.big`, `SpeechEnglish.big` and `Music.big` under `ZH_Generals\`. Of the Zero Hour archives' 3,523 WAVs, 2,572 are IMA ADPCM (format tag 17, 4 bits, most 44.1 kHz mono) and 951 PCM (16 bits, most 22.05 kHz mono); the music is 56 MP3s, 192 MB.
- **An event's files are named by their stem.** `AudioSettings.ini` gives the root `Data\Audio`, the folders `Sounds`, `Tracks` and `Speech` and the extension `wav`; a sound's path is `Data\Audio\Sounds\<stem>.wav`, its language's folder `Sounds\English\` taken first when it holds the file (`AudioEventRTS::adjustForLocalization`), and a track's `Data\Audio\Tracks\<Filename>`.
- **What the events name.** `SoundEffects.ini` holds 743 events naming 1,816 stems, 135 MB of files, 25 stems of no file; `Voice.ini` 666 events naming 2,074 stems, 122 MB, 19 of no file; `Speech.ini` 2,568 dialog events, the campaign's and EVA's.

## An event

- **An event is a list of files and how to play them.** `AudioEvent` takes `DefaultSoundEffect`'s fields first: `Volume` 100 %, `MinVolume` 40 %, `Priority` high, `Control` random, `Limit` 4, `MinRange` 175, `MaxRange` 800, `Type` ui player. Its fields: `Sounds`, the stems; `Volume` and `VolumeShift`, a percent, each play drawing its volume between `1 + VolumeShift` and 1 of `Volume`; `PitchShift min max`, in percent, a play's pitch drawn between them; `Delay`; `Limit`; `LoopCount`; `Priority`; `Type`; `Control`; `MinRange` and `MaxRange`; `LowPassCutoff`; and `Attack`, `Decay` and `SoundsNight`, `SoundsEvening`, `SoundsMorning` (`AudioEventInfo::m_audioEventInfo`).
- **A play draws its file.** Of `Control = random`, a file drawn at random, the next one when it draws the last one played and the list holds more than 2; else each play the next file in turn (`AudioEventRTS::generateFilename`).
- **`Type` says where and to whom.** `ui` plays flat, `world` at its place, `global` at a range of 5,000 to 500,000; `player`, `allies`, `enemies` or `everyone` says who hears it, by its owner's relation to the local player; a sound of no `shrouded` is not heard where the local player's shroud lies (`AudioManager::shouldPlayLocally`). Of the sound effects, 403 are `world shrouded everyone`, 190 `world everyone`, 63 `ui player`; voices are `ui voice player`.
- **A limit refuses or interrupts.** At most `Limit` plays of an event at once, 0 for none; a play past it is dropped, or, of `Control = interrupt`, stops the oldest (`MilesAudioManager::doesViolateLimit`). Past the device's samples, 25 positional and 4 flat (`SampleCount3D`, `SampleCount2D`), a play of a higher priority stops one of the lowest.

## Where a sound is heard

- **The microphone stands between the ground and the camera.** From the camera's ground point toward the camera, up to 50 above the ground (`MicrophoneDesiredHeightAboveTerrain`) or a third of the way (`MicrophoneMaxPercentageBetweenGroundAndCamera`), whichever is nearer, facing the camera's heading (`AudioManager::update`).
- **A near camera is louder.** Positional sounds play at 80 % (`ZoomSoundVolumePercentageAmount` 20 %), rising linearly to 100 % as the camera comes from 425 to 130 of the microphone (`ZoomMaxDistance`, `ZoomMinDistance`).
- **The release game falls off as the inverse of distance and cuts at the range's end.** A positional play's volume is its volume times its class's, times `MinRange / distance` past `MinRange`, and 0 at `MaxRange` and past (`MilesAudioManager::getEffectiveVolume` before GeneralsX's change). It passes Miles `AIL_set_3D_sample_distances(sample, MinRange, MaxRange)`, whose parameters are `max_dist, min_dist`, so Miles' own fall-off starts past `MaxRange`, where the play is cut already.
- **TheSuperHackers fade to the range's end.** A change of 3 March 2026 (#2369) drops the inverse fall-off for `1 − ((distance − MinRange) / (MaxRange − MinRange))⁴`, on by default and off by `Use3DSoundRangeVolumeFade = No`, "as originally intended", so a sound no longer stops at `MaxRange`.
- **Each class has its volume.** Sounds 80 %, positional sounds 80 %, speech 70 %, music 55 % (`DefaultSoundVolume`, `Default3DSoundVolume`, `DefaultSpeechVolume`, `DefaultMusicVolume`); flat sounds 10 % under positional ones (`Relative2DVolume`).

## What plays a sound

- **A weapon's fire.** 231 weapons give a `FireSound`, played at the firer as it fires, 37 of them a `FireSoundLoopTime` that loops it while the weapon fires again.
- **An effect list.** 322 `Sound` nuggets of `FXList.ini` name an event, played at the effect's place, as a hit, a detonation and most deaths play.
- **A unit's voices.** An object names `VoiceSelect` (620 lines), `VoiceMove` (386), `VoiceAttack` (380), `VoiceCreated` and others; `SoundMoveStart` (277) and `SoundAmbient` (312) and the `UnitSpecificSounds` block name its other sounds.
- **The voice of a selection and an order.** `pickAndPlayUnitVoiceResponse` walks the selected drawables in their order: a new selection plays the first one's `VoiceSelect`, a move or an attack move the first's `VoiceMove`, an attack the first's `VoiceAttack`, a special case, as a crush or a build, replacing it.
- **A unit made.** `ProductionUpdate` plays a trained unit's `VoiceCreated` for its owner as it leaves.
- **Speech.** EVA's announcements (`Eva.ini`) and the radar's, the money's and the UI's sounds (`MiscAudio.ini`).

## Music

- **A track is a file.** `Music.ini` names 68 tracks, each of `Filename`; `Game_USA_01` to `11`, `Game_CHI_01` to `10` and `Game_GLA_01` to `11` play in a match, `Game_Silent` 60 s of silence.
- **The skirmish scripts play the player's faction's tracks.** `SkirmishScripts.scb` holds, for each faction, a cycle of its `Game_` tracks with `Game_Silent` between them, which restarts at its end, and events that play silence as the player loses an important building or a track as it builds its superweapon (its strings "China Music Track 2 Start", "Music Restart Cycle", "Music Nuke").

## OpenRA

- **Sounds are an actor's traits.** `AttackSounds` plays at an attack, `AmbientSound` loops, `Voiced` names a voice set whose voices `Voices.yaml` lists by order, and the first selected actor with a voice answers a selection and an order.

## What the engine holds

- **No sound.** The client has no audio; the workspace's Bevy has no `bevy_audio`.
- **Effects play lists of particle systems.** The client's effects play a list where a fire, a hit, a death phase or a projectile's end plays (`ParticleRun::play`); the import counts each `Sound` nugget as a loss.
- **bevy_audio decodes by rodio's features.** bevy_audio's `wav` takes `hound`, PCM alone, and its `symphonia-wav` rodio's `wav`, symphonia's WAV and PCM; IMA ADPCM needs rodio's `symphonia-adpcm`, and MP3 bevy_audio's `mp3`.
- **rodio's spatial sound falls off as the inverse square.** `Spatial` sets each ear's volume to `min(1, 1 / distance²)` times its side's share, with no range.
