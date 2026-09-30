use bevy_ecs::component::Component;
use campfire_math::{Num, Vec3};
use campfire_sim::{Position, SimComponent};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// How a unit attacks. An attack starts when its target is within `range` on the ground plane,
/// strikes `windup` ticks later for `damage`, and the next one starts `period` ticks after it at
/// the earliest. The windup is shorter than the period, so a strike lands before the next attack
/// may start.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AttackStats {
    range: Num,
    windup: u32,
    period: u32,
    damage: Num,
}

impl AttackStats {
    /// `None` for a negative range or damage, or a windup not shorter than the period.
    pub const fn new(range: Num, windup: u32, period: u32, damage: Num) -> Option<AttackStats> {
        if range.to_bits() < 0 || damage.to_bits() < 0 || windup >= period {
            return None;
        }
        Some(AttackStats {
            range,
            windup,
            period,
            damage,
        })
    }

    pub const fn range(self) -> Num {
        self.range
    }

    pub const fn windup(self) -> u32 {
        self.windup
    }

    pub const fn period(self) -> u32 {
        self.period
    }

    pub const fn damage(self) -> Num {
        self.damage
    }

    /// Whether an attack from `from` reaches `to`: within range on the ground plane, exactly.
    pub fn reaches(&self, from: Position, to: Position) -> bool {
        Vec3::ZERO.within(ground_offset(from, to), self.range)
    }
}

/// The offset from `from` to `to` on the ground plane: heights never count towards a range.
pub(crate) fn ground_offset(from: Position, to: Position) -> Vec3 {
    let offset = to.get() - from.get();
    Vec3::new(offset.x, Num::ZERO, offset.z)
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
            windup: u32,
            period: u32,
            damage: Num,
        }
        let fields = Fields::deserialize(deserializer)?;
        AttackStats::new(fields.range, fields.windup, fields.period, fields.damage)
            .ok_or_else(|| D::Error::custom("attack stats out of their limits"))
    }
}
