use std::f32::consts::FRAC_PI_2;

use campfire_client_files::{DeathFall, DeathLook, FallStage, GroundDeath};
use campfire_math::Num;

use super::*;
use crate::zero_hour::format::ini::audio_ini::AudioIni;
use crate::zero_hour::format::ini::fx_list_ini::samples::FX_LISTS;
use crate::zero_hour::format::ini::object_ini::ObjectFiles;
use crate::zero_hour::format::ini::particle_system_ini::samples::PARTICLE_SYSTEMS;
use crate::zero_hour::format::ini::weapon_ini::samples::WEAPONS;
use crate::zero_hour::format::wav::samples::wav;
use crate::zero_hour::production_bones::ProductionBones;

/// The audio of the sounds the samples name: the quiet list's hush and the cannon's fire, each a
/// file of its own.
const AUDIO: &str = "AudioEvent Hush\n  Sounds = hush\n  Volume = 50\n  PitchShift = 0 0\n  MinRange = 175\n  MaxRange = 800\n  Type = world everyone\nEnd\n\
                     AudioEvent TankCannonFire\n  Sounds = cannon\n  Volume = 100\n  PitchShift = 0 0\n  MinRange = 175\n  MaxRange = 800\n  Type = world everyone\nEnd\n";

/// The archives' files of the sounds of `AUDIO`, each a PCM WAV.
fn sound_file(path: &ArchivePath) -> Option<Vec<u8>> {
    let held = [
        "data\\audio\\sounds\\hush.wav",
        "data\\audio\\sounds\\cannon.wav",
    ];
    let pcm = wav([1, 1, 2, 16], 22_050, None, &[0, 0]);
    held.contains(&path.as_str()).then_some(pcm)
}

/// The effects of the systems of `systems` and the lists of `lists`, of a cap of 2,500, each
/// texture in `Art\Textures\` but those of names starting `Lost`, of no sound.
fn effects(systems: &str, lists: &str) -> EffectsImport {
    let audio = AudioIni::default();
    let mut sounds = SoundImport::new(&audio, None, |path: &ArchivePath| Ok(sound_file(path)));
    carried(systems, lists, &[], &mut sounds)
}

/// The effects of `effects`', the systems of `trails` carried by projectiles, the lists' sounds
/// of `sounds`.
fn carried<R: FnMut(&ArchivePath) -> Result<Option<Vec<u8>>, ZeroHourError>>(
    systems: &str,
    lists: &str,
    trails: &[&str],
    sounds: &mut SoundImport<'_, R>,
) -> EffectsImport {
    let mut particles = ParticleSystemIni::default();
    particles.read(systems.as_bytes()).unwrap();
    let mut effects = FxListIni::default();
    effects.read(lists.as_bytes()).unwrap();
    let sources = EffectSources {
        systems: &particles,
        lists: &effects,
        trails,
        cap: 2500,
        texture: |name: &str| {
            (!name.starts_with("Lost")).then(|| ArchivePath::of(&format!("art\\textures\\{name}")))
        },
    };
    EffectsImport::of(sources, sounds).unwrap()
}

fn name(text: &str) -> DeclaredName {
    DeclaredName::new(text).unwrap()
}

fn dropped(name: &str, cause: EffectDropCause) -> Dropped<EffectDropCause> {
    Dropped {
        name: name.to_owned(),
        cause,
    }
}

#[test]
fn a_list_s_systems_convert_on_the_engine_s_axes_and_what_the_slice_leaves_is_counted() {
    let effects = effects(PARTICLE_SYSTEMS, FX_LISTS);
    let file = &effects.file;
    assert_eq!((file.step(), file.cap()), (1.0 / 30.0, 2500));
    // The quiet list holds a sound of no event alone, so it goes; the hit's shake is counted.
    assert_eq!(
        effects.dropped,
        [dropped("FX_TankHit", EffectDropCause::ViewShake)]
    );
    assert_eq!(
        file.lists().keys().collect::<Vec<_>>(),
        [&name("fx_tankhit")]
    );
    // Of its event, the quiet list plays its sound alone, which the file holds.
    let mut audio = AudioIni::default();
    audio.read(AUDIO.as_bytes()).unwrap();
    let mut sounds = SoundImport::new(&audio, None, |path: &ArchivePath| Ok(sound_file(path)));
    let heard = carried(PARTICLE_SYSTEMS, FX_LISTS, &[], &mut sounds);
    let quiet = &heard.file.lists()[&name("fx_quiet")];
    assert_eq!(
        (quiet.systems.len(), &*quiet.sounds),
        (0, &[name("hush")][..])
    );
    assert_eq!(heard.file.sounds()[&name("hush")].volume, 0.5);
    let hit = &file.lists()[&name("fx_tankhit")].systems;
    assert_eq!(hit.len(), 2);
    assert_eq!(
        (&hit[0].system, hit[0].count, hit[0].delay),
        (&name("spark"), 1, None)
    );
    assert_eq!(hit[0].rotation, Quat::IDENTITY);
    // The smoke twice, 0 to 3 out, after 100 to 200 ms, 3 to 6 steps of 1/30 s.
    assert_eq!((&hit[1].system, hit[1].count), (&name("smoketrail"), 2));
    assert_eq!(
        hit[1].radius,
        Spread {
            low: 0.0,
            high: 3.0
        }
    );
    assert_eq!(
        hit[1].delay,
        Some(Spread {
            low: 3.0,
            high: 6.0
        })
    );

    // Both systems play, the smoke as the hit's and as the spark's slave; each texture once.
    assert_eq!(file.systems().len(), 2);
    assert_eq!(
        effects.textures,
        BTreeSet::from(
            ["art\\textures\\exsmoke.tga", "art\\textures\\exspark.tga"].map(ArchivePath::of)
        )
    );
    let smoke = &file.systems()[&name("smoketrail")];
    // `WEAPON_TRAIL`, the tenth priority from the lowest.
    assert_eq!((smoke.rank, smoke.blend), (Some(9), Blend::Alpha));
    assert_eq!(
        smoke.texture.as_str(),
        "client/textures/art/textures/exsmoke.ktx2"
    );
    // The game's drift up, its `z`, is the engine's `y`.
    assert_eq!(smoke.drift, Vec3::new(0.0, 0.1, 0.0));
    // Its alpha's fourth key, of frame 0, ends them, and its color's third.
    let alpha: Vec<(Spread, u32)> = smoke.alpha.iter().map(|key| (key.alpha, key.at)).collect();
    assert_eq!(
        alpha,
        [
            (Spread::ZERO, 0),
            (
                Spread {
                    low: 0.5,
                    high: 0.75
                },
                5
            ),
            (Spread::ZERO, 30)
        ]
    );
    let color: Vec<u32> = smoke.color.iter().map(|key| key.at).collect();
    assert_eq!(color, [0, 20]);
    assert_eq!(
        (smoke.velocity, smoke.volume, smoke.hollow),
        (
            EmissionVelocity::Cylindrical {
                radial: Spread {
                    low: 0.1,
                    high: 0.2
                },
                normal: Spread {
                    low: 1.0,
                    high: 1.5
                },
            },
            EmissionVolume::Cylinder {
                radius: 3.0,
                length: 2.0
            },
            true
        )
    );
    assert_eq!((&smoke.slave, &smoke.attached), (&None, &None));
    let spark = &file.systems()[&name("spark")];
    assert_eq!((spark.rank, spark.blend), (None, Blend::Add));
    // The slave's particles stand off by the smoke's own offset, the game's (4, 5, 6), on the
    // engine's axes (4, 6, −5), where the spark's own (1, 2, 3) offsets no slave of its own.
    assert_eq!(
        spark.slave,
        Some(SlaveSystem {
            system: name("smoketrail"),
            offset: Vec3::new(4.0, 6.0, -5.0),
        })
    );
    assert_eq!(spark.alpha.len(), 1);
}

#[test]
fn a_system_nothing_plays_goes_and_a_name_of_no_system_or_a_system_drawn_otherwise_drops() {
    let systems = "ParticleSystem Shown
  Shader = ALPHA
  Type = PARTICLE
  ParticleName = Shown.tga
  SlaveSystem = Nowhere
  PerParticleAttachedSystem = Streaky
  WindMotion = PingPong
  VelocityType = ORTHO
  VelOrthoX = 1 2
  VelOrthoY = 3 4
  VelOrthoZ = 5 6
  VolumeType = BOX
  VolBoxHalfSize = X:1 Y:2 Z:3
  Alpha1 = 0.5 0.5 10
  Alpha2 = 0.1 0.1 4
  Alpha3 = 0.2 0.2 10
  Alpha4 = 0.3 0.3 20
End
ParticleSystem Streaky
  Shader = ALPHA
  Type = STREAK
  ParticleName = Streak.tga
End
ParticleSystem Plain
  Type = PARTICLE
  ParticleName = Plain.tga
End
ParticleSystem Faded
  Shader = ADDITIVE
  Type = PARTICLE
  ParticleName = LostFaded.tga
End
ParticleSystem Unplayed
  Shader = ADDITIVE
  Type = PARTICLE
  ParticleName = Unplayed.tga
End
";
    let lists = "FXList FX_Mixed
  ParticleSystem
    Name = Shown
    RotateY = 90
    RotateZ = 90
  End
  ParticleSystem
    Name = Missing
  End
  ParticleSystem
    Name = Plain
  End
  ParticleSystem
    Name = Faded
  End
  ParticleSystem
    Name = Shown
    Count = 0
  End
  TerrainScorch
  End
End
";
    let effects = effects(systems, lists);
    // Each system drops by its cause as the systems read, then each list's nugget; the system
    // nothing plays goes and is no loss.
    assert_eq!(
        effects.dropped,
        [
            dropped("Faded", EffectDropCause::Texture),
            dropped("Plain", EffectDropCause::Shader),
            dropped("Streaky", EffectDropCause::Kind),
            dropped("FX_Mixed", EffectDropCause::NoSystem),
            dropped("FX_Mixed", EffectDropCause::TerrainScorch),
            dropped("Shown", EffectDropCause::NoSystem),
            dropped("Shown", EffectDropCause::Wind),
        ]
    );
    let file = &effects.file;
    assert_eq!(file.systems().keys().collect::<Vec<_>>(), [&name("shown")]);
    // The shown system's entry alone, of count 0 none; its slave of no system and its
    // particles' streaks go.
    let mixed = &file.lists()[&name("fx_mixed")].systems;
    assert_eq!(mixed.len(), 1);
    let shown = &file.systems()[&name("shown")];
    assert_eq!((&shown.slave, &shown.attached), (&None, &None));
    // The game turns the entry's frame 90° about its y, then 90° about its z: about the engine's
    // −z, then its y. A point on the game's x turns about its z to its y, the engine's −z, which
    // the turn about its y keeps.
    let turned = mixed[0].rotation * Vec3::X;
    assert!(turned.abs_diff_eq(Vec3::NEG_Z, 1e-6), "{turned}");
    let check = Quat::from_rotation_z(-FRAC_PI_2) * Quat::from_rotation_y(FRAC_PI_2);
    assert!(mixed[0].rotation.abs_diff_eq(check, 1e-6));
    // The game's speeds along its y, 3 to 4, are the engine's along −z, −4 to −3; its box's half
    // sizes keep their sign.
    assert_eq!(
        (shown.velocity, shown.volume),
        (
            EmissionVelocity::Ortho {
                x: Spread {
                    low: 1.0,
                    high: 2.0
                },
                y: Spread {
                    low: 5.0,
                    high: 6.0
                },
                z: Spread {
                    low: -4.0,
                    high: -3.0
                },
            },
            EmissionVolume::Box {
                half: Vec3::new(1.0, 3.0, 2.0)
            }
        )
    );
    // The keys by their frames, of frame 10 the later, 0.2.
    let alpha: Vec<(f32, u32)> = shown
        .alpha
        .iter()
        .map(|key| (key.alpha.low, key.at))
        .collect();
    assert_eq!(alpha, [(0.1, 4), (0.2, 10), (0.3, 20)]);
}

#[test]
fn a_projectile_s_trail_plays_and_each_weapon_and_death_names_the_effects_the_file_holds() {
    // A system no list names, which a projectile carries, plays; with none, it goes.
    let exhaust = format!(
        "{PARTICLE_SYSTEMS}ParticleSystem Exhaust\n  Shader = ALPHA\n  Type = PARTICLE\n  ParticleName = Puff.tga\nEnd\n"
    );
    let trimmed = effects(&exhaust, FX_LISTS);
    assert!(!trimmed.file.systems().contains_key(&name("exhaust")));
    let mut audio = AudioIni::default();
    audio.read(AUDIO.as_bytes()).unwrap();
    let mut sounds = SoundImport::new(&audio, None, |path: &ArchivePath| Ok(sound_file(path)));
    let mut effects = carried(&exhaust, FX_LISTS, &["Exhaust"], &mut sounds);
    assert!(effects.file.systems().contains_key(&name("exhaust")));

    // The cannon fires `FX_TankFire`, which the lists do not hold, and its shell hits with
    // `FX_TankHit`, which its shell's look names, of no exhaust; the rifle, a shot, plays
    // nothing, and names no look. The cannon converts to the action of each set it serves, and
    // each plays its look (09-zero-hour.md, A look follows its converted action).
    let mut weapons = WeaponIni::default();
    weapons.read(WEAPONS.as_bytes()).unwrap();
    let shell = name("tank_shell");
    let [starting, upgraded, rifle] = [
        "weapon_tankcannon_starting",
        "weapon_tankcannon_upgraded",
        "weapon_rangerrifle",
    ]
    .map(name);
    let fired_shells = [&starting, &upgraded].map(|action| ConvertedShot {
        action,
        shot: WeaponShot::Projectile(&shell),
    });
    let rifle_shot = [ConvertedShot {
        action: &rifle,
        shot: WeaponShot::Shot,
    }];
    let projectile = |weapon: &str| match weapon {
        "TankCannon" => &fired_shells[..],
        "RangerRifle" => &rifle_shot[..],
        _ => &[],
    };
    let mut units = BTreeMap::new();
    let looks = effects
        .action_looks(&weapons, projectile, &mut sounds, &mut units)
        .unwrap();
    // The cannon's fire plays its sound, whose loop over 80 ms is counted.
    let cannon = |look: ActionLook| {
        BTreeMap::from([(starting.clone(), look.clone()), (upgraded.clone(), look)])
    };
    let fired = ActionLook {
        fire_sound: Some(name("tankcannonfire")),
        ..ActionLook::default()
    };
    assert_eq!(looks, cannon(fired.clone()));
    assert_eq!(
        (&units[&shell].end, &units[&shell].trail),
        (&Some(name("fx_tankhit")), &None)
    );
    // A shot of no projectile names its hit on its action; a weapon the import does not
    // convert names none.
    let cannon_shots = [&starting, &upgraded].map(|action| ConvertedShot {
        action,
        shot: WeaponShot::Shot,
    });
    let mut units = BTreeMap::new();
    let shot = |weapon: &str| match weapon {
        "TankCannon" => &cannon_shots[..],
        _ => &[],
    };
    let looks = effects
        .action_looks(&weapons, shot, &mut sounds, &mut units)
        .unwrap();
    assert_eq!(
        looks,
        cannon(ActionLook {
            hit: Some(name("fx_tankhit")),
            ..fired
        })
    );
    assert!(units.is_empty());
    let none = effects.action_looks(&weapons, |_| &[], &mut sounds, &mut units);
    assert!(none.unwrap().is_empty());
    let sounded = sounds.finish().dropped;
    assert_eq!(
        sounded,
        [Dropped {
            name: "TankCannonFire".to_owned(),
            cause: SoundDropCause::Loop,
        }]
    );

    // A tank's death plays at its start its first `FXListDie`'s list and its slow death's first
    // `INITIAL` list, at its middle its `MIDPOINT`'s, which the file lacks, and at its end the
    // first of its two `FINAL` lists, each the file holds; a look of no object, or an object of
    // no look, names none. Its tread throws the game's dirt, which the file lacks, and lays the
    // track marks of a texture an archive holds, which the import converts. The phase's other
    // list, its weapon and its tread's systems are counted, once each; its hulk is the wrecks'
    // import's. A jet's death plays at its start its `FXListDie`'s list alone, and its fall,
    // which the wrecks' import named, plays its own at its start, its hit and on the ground; its
    // slow death's own phase is counted.
    let tank = "Object Tank\n  Draw = W3DTankDraw D\n    TrackMarks = EXTnkTrack.tga\n  End\n  \
                Behavior = FXListDie M\n    DeathFX = FX_TankHit\n  End\n  \
                Behavior = SlowDeathBehavior S\n    FX = INITIAL FX_TankHit\n    \
                FX = MIDPOINT FX_Missing\n    OCL = MIDPOINT OCL_Hulk\n    OCL = FINAL OCL_Turret\n    \
                FX = FINAL FX_TankHit\n    FX = FINAL FX_Other\n    Weapon = FINAL Blast\n  End\nEnd\n";
    let jet = "Object Jet\n  Behavior = FXListDie M\n    DeathFX = FX_TankHit\n  End\n  \
               Behavior = JetSlowDeathBehavior J\n    FXInitialDeath = FX_TankHit\n    \
               FXHitGround = FX_TankHit\n    FXOnGroundDeath = FX_TankHit\n    \
               FX = INITIAL FX_TankHit\n  End\nEnd\n";
    let rock =
        "Object Rock\n  Draw = W3DModelDraw D\n    TrackMarks = EXTnkTrack.tga\n  End\nEnd\n";
    let mut objects = ObjectFiles::default();
    objects
        .read(format!("{tank}{jet}{rock}").as_bytes())
        .unwrap();
    let objects = objects.finish();
    let units = BTreeMap::from([
        (name("tank"), ClientUnit::new(Vec::new())),
        (name("rock"), ClientUnit::new(Vec::new())),
        (name("jet"), ClientUnit::new(Vec::new())),
    ]);
    let mut looks = ImportedLooks {
        units,
        conditions: BTreeMap::new(),
        objects: BTreeMap::new(),
        bones: BTreeMap::new(),
        track_widths: BTreeMap::from([(name("tank"), 17.5)]),
        points: ProductionBones::default(),
        fire_points: BTreeMap::new(),
    };
    let units = &mut looks.units;
    let fall = DeathFall {
        gravity: Num::ONE,
        roll: Num::ZERO,
        spiral: None,
        start: FallStage::default(),
        hit: FallStage::default(),
        blow_up_after: Num::ZERO,
        blow_up: FallStage::default(),
        ground: Some(GroundDeath {
            height: Num::ONE,
            stage: FallStage::default(),
        }),
    };
    units.get_mut(&name("jet")).unwrap().death.fall = Some(fall.clone());
    let before = effects.dropped.len();
    let track = ArchivePath::of("art\\textures\\extnktrack.tga");
    let found = |name: &str| (name == "EXTnkTrack.tga").then(|| track.clone());
    effects.name_deaths(&objects, found, &mut looks).unwrap();
    let units = &looks.units;
    assert!(effects.textures.contains(&track));
    // The tank's track is as wide as its model's tread bones say; the rock's, of none, the
    // game's 14.
    let laid = |width: &str| TreadLook {
        systems: Vec::new(),
        track: Some(TrackLook {
            texture: game_path::texture(&track).unwrap(),
            width: width.parse().unwrap(),
        }),
    };
    assert_eq!(units[&name("tank")].tread, laid("17.5"));
    assert_eq!(units[&name("rock")].tread, laid("14"));
    let hit = name("fx_tankhit");
    assert_eq!(
        units[&name("tank")].death,
        DeathLook {
            start: vec![hit.clone(), hit.clone()],
            end: vec![hit.clone()],
            ..DeathLook::NONE
        }
    );
    let played = FallStage {
        effects: vec![hit.clone()],
        wrecks: Vec::new(),
    };
    assert_eq!(
        units[&name("jet")].death,
        DeathLook {
            start: vec![hit],
            fall: Some(DeathFall {
                start: played.clone(),
                hit: played.clone(),
                ground: Some(GroundDeath {
                    height: Num::ONE,
                    stage: played,
                }),
                ..fall
            }),
            ..DeathLook::NONE
        }
    );
    assert!(units[&name("rock")].death.is_empty());
    let tank_losses = [
        EffectDropCause::DeathChoice,
        EffectDropCause::DeathWeapon,
        EffectDropCause::NoSystem,
    ]
    .map(|cause| dropped("Tank", cause));
    let mut losses = vec![dropped("Jet", EffectDropCause::FallPhase)];
    losses.extend(tank_losses);
    let mut seen = effects.dropped[before..].to_vec();
    seen.sort();
    losses.sort();
    assert_eq!(seen, losses);
}
