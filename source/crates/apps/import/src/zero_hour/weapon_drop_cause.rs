use campfire_common::EnumAll;
use serde::{Deserialize, Serialize};

use crate::zero_hour::error::TimeError;

/// Why the import drops a weapon (09-zero-hour.md, A weapon converts when it hits one target at
/// once; 09-zero-hour.md, The import converts shells, splash, scatter, pitch, turrets
/// and slow weapons; 09-zero-hour.md, Missiles the steer cannot hold stay dropped).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll, Serialize, Deserialize)]
pub enum WeaponDropCause {
    /// No file defines it.
    NoWeapon,
    /// It fires a `ProjectileObject` of neither a `DumbProjectileBehavior` nor a
    /// `MissileAIUpdate` of a `THRUST` locomotor, or one no file defines.
    NoFlight,
    /// It draws its projectiles as a stream, a `ProjectileStreamName`, as a flamethrower or a
    /// toxin sprayer.
    Stream,
    /// Its missile's fuel lasts a frame at most, so it bursts where it leaves, as the toxin
    /// truck's contamination.
    Sprayer,
    /// Its missile ignites after an `IgnitionDelay`.
    Ignition,
    /// Its missile detonates as its fuel runs out, `DetonateOnNoFuel`.
    FuelBurst,
    /// Its missile kills units garrisoned in what it strikes, a `GarrisonHitKillCount`.
    GarrisonKill,
    /// Its missile's fuel lasts for ever, a `FuelLifetime` of 0.
    NoFuel,
    /// Its projectile creates objects where it detonates, a `ProjectileDetonationOCL`, as a
    /// poison field.
    DetonationObjects,
    /// It damages a `PrimaryDamageRadius` or a `SecondaryDamageRadius` with no projectile, which
    /// alone ends where a splash is dealt.
    Radius,
    /// It aims within a `ScatterRadius`, or a `ScatterRadiusVsInfantry` at infantry, with no
    /// projectile, which alone flies to a point it scatters to.
    Scatter,
    /// Its radius spares the airborne, `NOT_AIRBORNE`, or damages a set of relations no filter
    /// names.
    Affects,
    /// Its shell detonates on shrubbery, projectiles or missiles, which are no units of the
    /// engine's.
    Collides,
    /// It may hit neither the ground nor a vehicle in the air, `AntiGround = No` and no
    /// `AntiAirborneVehicle`.
    NoTarget,
    /// It kills its own unit, `RadiusDamageAffects` of `SUICIDE`.
    Suicide,
    /// It deals its damage at its own place, `DamageDealtAtSelfPosition`.
    AtSelf,
    /// Its continuous fire's stages change a field other than its rate of fire, or coast for no
    /// time.
    ContinuousFire,
    /// Its bonus of `PLAYER_UPGRADE` changes a field other than its damage.
    UpgradeBonus,
    /// It keeps a `MinimumAttackRange` not within its `AttackRange`.
    MinRange,
    /// Its `DelayBetweenShots` is a range the game draws from, and its clip holds more than one
    /// shot.
    RandomDelay,
    /// Its period is no frame, which the engine refuses: its `DelayBetweenShots`, or the
    /// `ClipReloadTime` of a clip of one shot.
    NoDelay,
    /// Its `DamageType` takes no life: a status, a subdual, or the kill of a pilot or a garrison.
    NoLife,
    /// Its `DamageType` is `HEALING`, which gives its target life.
    Healing,
    /// A number of it is below 0, not finite, or past what the engine holds, or its pitches
    /// reach no target.
    BadNumber,
    /// A name of its action or of a stage of it passes the bytes a declared name holds.
    LongName,
}

/// A time the import writes none of is a number of no worth to the engine.
impl From<TimeError> for WeaponDropCause {
    fn from(_: TimeError) -> Self {
        Self::BadNumber
    }
}
