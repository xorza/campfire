use campfire_common::EnumAll;

/// Why the import drops a weapon (09-zero-hour.md, A weapon converts when it hits one target at
/// once; 09-zero-hour.md, The import converts shells, splash, scatter, pitch, turrets
/// and slow weapons; 09-zero-hour.md, Missiles the steer cannot hold stay dropped).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll)]
pub enum WeaponDrop {
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
    /// It may not hit the ground, `AntiGround = No`.
    NotGround,
    /// It kills its own unit, `RadiusDamageAffects` of `SUICIDE`.
    Suicide,
    /// It deals its damage at its own place, `DamageDealtAtSelfPosition`.
    AtSelf,
    /// Its continuous fire's stages change a field other than its rate of fire, or coast for no
    /// time.
    ContinuousFire,
    /// It waits a `PreAttackDelay` before it strikes.
    PreAttack,
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
}

/// The game's logic frames a millisecond, `LOGICFRAMES_PER_MSEC_REAL`, in the game's `f32`.
const FRAMES_PER_MS: f32 = 30.0 / 1000.0;

/// The milliseconds the engine turns into the frames the game turns `ms` into: the game's
/// `ceil(ms × 0.03)` in its `f32` (`INI::parseDurationUnsignedInt`, `parseShotDelay`), as the
/// import's rule of times writes it (09-zero-hour.md, Thirty frames a second, the original's
/// times), `⌊frames × 100 / 3⌋`, which the engine's rounding up turns back into the same frames;
/// a time below 0 is no number.
pub(crate) fn game_ms(ms: i32) -> Result<u64, WeaponDrop> {
    if ms < 0 {
        return Err(WeaponDrop::BadNumber);
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the game's frames, a whole count at least 0 the `f32` holds exactly"
    )]
    let frames = (ms as f32 * FRAMES_PER_MS).ceil() as u64;
    Ok(frames * 100 / 3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_is_written_as_the_milliseconds_of_the_games_frames() {
        // 1 ms is ⌈0.03⌉ = 1 frame, 33 ms; 2,000 ms are 60 frames, 2,000; a time below 0 is no
        // number.
        assert_eq!(game_ms(1), Ok(33));
        assert_eq!(game_ms(2000), Ok(2000));
        assert_eq!(game_ms(-1), Err(WeaponDrop::BadNumber));
    }
}
