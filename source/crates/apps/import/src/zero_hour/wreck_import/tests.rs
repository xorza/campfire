use super::*;
use crate::zero_hour::format::ini::object_ini::ObjectFiles;

/// A tank whose death's middle throws its hulk and debris and its end its turret, a list no file
/// holds and none; its hulk; the game's debris; a jet and a helicopter that fall.
const OBJECTS: &str = "Object Tank
  Behavior = SlowDeathBehavior S
    OCL = MIDPOINT OCL_Death
    OCL = FINAL OCL_Turret
    OCL = FINAL OCL_Missing
  End
End
Object DeadTankHulk
  Draw = W3DModelDraw D
    ConditionState = NONE
      Model = AVHulk
    End
  End
  KindOf = NO_COLLIDE HULK
  Behavior = PhysicsBehavior P
    Mass = 100.0
  End
  Behavior = LifetimeUpdate L
    MinLifetime = 1500
    MaxLifetime = 1600
  End
  Behavior = SlowDeathBehavior S
    SinkDelay = 1500
    SinkRate = 2
    DestructionDelay = 8000
  End
End
Object GenericDebris
  Behavior = SlowDeathBehavior S
    SinkDelay = 1000
    SinkRate = 1
    DestructionDelay = 2000
  End
End
Object Soldier
End
Object Jet
  Behavior = JetSlowDeathBehavior J
    DestructionDelay = 99999999
    RollRate = 0.2
    FallHowFast = 110.0%
    FXInitialDeath = FX_Start
    OCLInitialDeath = OCL_Death
    FXSecondary = FX_Second
    OCLHitGround = OCL_Turret
    DelayFinalBlowUpFromHitGround = 200
    OCLFinalBlowUp = None
    FXOnGroundDeath = FX_Ground
  End
End
Object Heli
  Locomotor = SET_NORMAL Rotors
  Behavior = PhysicsBehavior P
    Mass = 50.0
  End
  Behavior = HelicopterSlowDeathBehavior H
    SpiralOrbitTurnRate = 140.0
    SpiralOrbitForwardSpeed = 350.0
    SpiralOrbitForwardSpeedDamping = .9999
    MaxBraking = 190
    MinSelfSpin = 100
    MaxSelfSpin = 300
    SelfSpinUpdateDelay = 100
    SelfSpinUpdateAmount = 10
    FallHowFast = 12.0%
    DelayFromGroundToFinalDeath = 1500
    OCLBlade = OCL_Turret
  End
End
";

const LISTS: &str = "ObjectCreationList OCL_Death
  CreateObject
    ObjectNames = DeadTankHulk
    Disposition = SEND_IT_FLYING
    DispositionIntensity = 0.1
  End
  CreateDebris
    ModelNames = GXDebris GXBroken
    Offset = X:17.5 Y:2.0 Z:10.0
    Mass = 5.0
    Count = 2
    Disposition = SEND_IT_FLYING
    DispositionIntensity = 2.5
  End
  DeliverPayload
  End
End
ObjectCreationList OCL_Turret
  CreateDebris
    ModelNames = AVTurret
    Mass = 50.0
    Disposition = RANDOM_FORCE INHERIT_VELOCITY
    MinForceMagnitude = 100
    MaxForceMagnitude = 150
    MinForcePitch = 85
    MaxForcePitch = 90
    SpinRate = 300
  End
  CreateObject
    ObjectNames = Soldier
  End
End
";

/// The helicopter's hover, which brakes harder than its death's `MaxBraking` and accelerates
/// less damaged.
const LOCOMOTORS: &str = "Locomotor Rotors
  Surfaces = AIR
  Acceleration = 60
  AccelerationDamaged = 50
  Braking = 240
  Appearance = HOVER
End
";

fn num(value: f32) -> Num {
    game_float::to_num(value).unwrap()
}

fn model(name: &str) -> PackagePath {
    game_path::model(name).unwrap()
}

#[test]
fn a_death_throws_its_lists_hulks_and_debris_and_an_aircraft_falls() {
    let mut files = ObjectFiles::default();
    files.read(OBJECTS.as_bytes()).unwrap();
    let objects = files.finish();
    let mut lists = CreationListIni::default();
    lists.read(LISTS.as_bytes()).unwrap();
    let name = |text: &str| DeclaredName::new(text).unwrap();
    let mut looks: BTreeMap<DeclaredName, ClientUnit> = ["tank", "jet", "heli"]
        .map(|look| (name(look), ClientUnit::new(Vec::new())))
        .into();
    // Every model writes but `gxbroken`.
    let mut written = |model: &str| Ok(model != "gxbroken");
    let mut locomotors = LocomotorIni::default();
    locomotors.read(LOCOMOTORS.as_bytes()).unwrap();
    let losses = WreckImport::new(&objects, &lists, &locomotors, -64.0)
        .unwrap()
        .name(&mut written, &mut looks)
        .unwrap();

    // The hulk: flung by 4 × 0.1 across and 3 × 0.1 up, from 33 % of it, over its mass of 100, a
    // frame's change of velocity, 30 a second; spun at π/32 × 0.1 a frame; living the mean of 1.5 s
    // and 1.6 s, then sinking 2 a second after 1.5 s, gone at 8 s.
    let hulk = Wreck {
        models: vec![model("avhulk")],
        count: 1,
        offset: [Num::ZERO; 3],
        launch: WreckLaunch::Fling {
            across: num(4.0 * 0.1 / 100.0 * 30.0),
            up: [
                num(3.0 * 0.1 * 0.33 / 100.0 * 30.0),
                num(3.0 * 0.1 / 100.0 * 30.0),
            ],
        },
        spin: num(PI / 32.0 * 0.1 * 30.0),
        sink: WreckSink {
            // The mean of 1,500 and 1,600 ms is 1,550, ⌈46.5⌉ = 47 frames of the game, 1,566 ms,
            // as the sim counts a death's delay.
            life: Some(num(1566.0 / 1000.0)),
            delay: num(1.5),
            rate: num(2.0),
            gone: num(8.0),
        },
    };
    // The debris: two pieces of the model that writes, its offset on the engine's axes, `y` up,
    // flung by 2.5 over 5; resting, it sinks as the game's debris, 1 a second after 1 s, gone at
    // 2 s.
    let debris = Wreck {
        models: vec![model("gxdebris")],
        count: 2,
        offset: [num(17.5), num(10.0), num(-2.0)],
        launch: WreckLaunch::Fling {
            across: num(4.0 * 2.5 / 5.0 * 30.0),
            up: [
                num(3.0 * 2.5 * 0.33 / 5.0 * 30.0),
                num(3.0 * 2.5 / 5.0 * 30.0),
            ],
        },
        spin: num(PI / 32.0 * 2.5 * 30.0),
        sink: WreckSink {
            life: None,
            delay: num(1.0),
            rate: num(1.0),
            gone: num(2.0),
        },
    };
    // The turret: forced by 100 to 150 over 50, at 85° to 90°, spun at 300° a second.
    let turret = Wreck {
        models: vec![model("avturret")],
        count: 1,
        offset: [Num::ZERO; 3],
        launch: WreckLaunch::Force {
            speed: [num(100.0 / 50.0 * 30.0), num(150.0 / 50.0 * 30.0)],
            pitch: [num(85.0_f32.to_radians()), num(90.0_f32.to_radians())],
        },
        spin: num(300.0_f32.to_radians()),
        sink: debris.sink,
    };
    let death = WreckList(vec![hulk, debris]);
    let turrets = WreckList(vec![turret]);
    let tank = &looks[&name("tank")].death;
    assert_eq!(
        tank.wrecks,
        DeathWrecks {
            start: Vec::new(),
            middle: vec![death.clone()],
            end: vec![turrets.clone()],
        }
    );
    assert_eq!(tank.fall, None);

    // The jet: its fall's start throws the death's list; it falls at 110 % of gravity, rolling
    // 0.2 a frame, its hit throws the turret's list, and it blows up 200 ms after, of no list; it
    // dies on the ground at most 9 × 64 / 30² = 0.64 above it, which throws no list.
    let jet = &looks[&name("jet")].death;
    assert_eq!(jet.wrecks, DeathWrecks::NONE);
    let stage = |wrecks: Vec<WreckList>| FallStage {
        effects: Vec::new(),
        wrecks,
    };
    assert_eq!(
        jet.fall,
        Some(DeathFall {
            gravity: num(110.0 / 100.0),
            roll: num(0.2 * 30.0),
            spiral: None,
            start: stage(vec![death]),
            hit: stage(vec![turrets]),
            blow_up_after: num(200.0 / 1000.0),
            blow_up: stage(Vec::new()),
            ground: Some(GroundDeath {
                height: num(9.0 * 64.0 / (30.0 * 30.0)),
                stage: stage(Vec::new()),
            }),
        })
    );
    // The helicopter spirals by a push of 350 a frame over its mass of 50 each frame, 350 × 30 ÷
    // 50 = 210 a second squared, along a way that turns 140° a second, kept 0.9999 each step; its
    // hover brakes by the least of its 240 and its death's 190, and accelerates by its damaged 50;
    // it spins from 100° to 300° a second by 10° each 0.1 s, falls at 12 % of gravity, and blows
    // up 1.5 s after it hits.
    let heli = &looks[&name("heli")].death;
    assert_eq!(
        heli.fall,
        Some(DeathFall {
            gravity: num(12.0 / 100.0),
            roll: Num::ZERO,
            spiral: Some(Spiral {
                push: num(210.0),
                turn: num(140.0_f32.to_radians()),
                fade: num(0.9999),
                braking: num(190.0),
                acceleration: num(50.0),
                spin: SelfSpin {
                    least: num(100.0_f32.to_radians()),
                    most: num(300.0_f32.to_radians()),
                    change: num(10.0_f32.to_radians()),
                    every: num(100.0 / 1000.0),
                },
            }),
            start: stage(Vec::new()),
            hit: stage(Vec::new()),
            blow_up_after: num(1500.0 / 1000.0),
            blow_up: stage(Vec::new()),
            ground: None,
        })
    );
    // What each list and fall leaves, once a cause for each name.
    let causes = |name: &str| -> Vec<EffectDropCause> {
        (losses.iter())
            .filter(|dropped| dropped.name == name)
            .map(|dropped| dropped.cause)
            .collect()
    };
    assert_eq!(
        causes("OCL_Death"),
        [EffectDropCause::WreckModel, EffectDropCause::CreationNugget]
    );
    assert_eq!(
        causes("OCL_Turret"),
        [
            EffectDropCause::WreckLaunch,
            EffectDropCause::CreationObject
        ]
    );
    assert_eq!(causes("Tank"), [EffectDropCause::NoCreationList]);
    assert_eq!(causes("Jet"), [EffectDropCause::FallMoment]);
    assert_eq!(causes("Heli"), [EffectDropCause::FallMoment]);
}
