use bevy_ecs::component::Component;
use campfire_math::Num;
use campfire_sim::{Position, SimComponent, Ticks};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// How a unit attacks. An attack starts when its target is within `range` on the ground plane,
/// strikes `windup` ticks later for `damage`, and the next one starts `period` ticks after it at
/// the earliest. The windup is shorter than the period, so a strike lands before the next attack
/// may start. A ranged attack fires a projectile of `projectile_speed` a tick instead, in a
/// match with projectiles.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AttackStats {
    range: Num,
    windup: Ticks,
    period: Ticks,
    damage: Num,
    projectile_speed: Option<Num>,
}

impl AttackStats {
    /// `None` for a negative range or damage, or a windup not shorter than the period.
    pub const fn new(range: Num, windup: Ticks, period: Ticks, damage: Num) -> Option<AttackStats> {
        if range.to_bits() < 0 || damage.to_bits() < 0 || windup.get() >= period.get() {
            return None;
        }
        Some(AttackStats {
            range,
            windup,
            period,
            damage,
            projectile_speed: None,
        })
    }

    /// The same attack, ranged: it fires a projectile that flies `speed` a tick. `None` unless
    /// the speed is positive.
    pub const fn ranged(self, speed: Num) -> Option<AttackStats> {
        if speed.to_bits() <= 0 {
            return None;
        }
        Some(AttackStats {
            projectile_speed: Some(speed),
            ..self
        })
    }

    pub const fn range(self) -> Num {
        self.range
    }

    pub const fn windup(self) -> Ticks {
        self.windup
    }

    pub const fn period(self) -> Ticks {
        self.period
    }

    pub const fn damage(self) -> Num {
        self.damage
    }

    pub const fn projectile_speed(self) -> Option<Num> {
        self.projectile_speed
    }

    /// Whether an attack from `from` reaches `to`: within range on the ground plane, exactly.
    pub fn reaches(&self, from: Position, to: Position) -> bool {
        from.within_ground(to, self.range)
    }
}

impl SimComponent for AttackStats {
    const NAME: &'static str = "combat.attack_stats";
}

/// A snapshot is untrusted, so stats `new` refuses fail to decode.
impl<'de> Deserialize<'de> for AttackStats {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<AttackStats, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            range: Num,
            windup: Ticks,
            period: Ticks,
            damage: Num,
            projectile_speed: Option<Num>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let melee = AttackStats::new(fields.range, fields.windup, fields.period, fields.damage);
        let stats = match fields.projectile_speed {
            None => melee,
            Some(speed) => melee.and_then(|melee| melee.ranged(speed)),
        };
        stats.ok_or_else(|| D::Error::custom("attack stats out of their limits"))
    }
}
