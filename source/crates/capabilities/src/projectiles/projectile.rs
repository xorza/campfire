use bevy_ecs::component::Component;
use campfire_math::Num;
use campfire_sim::{SimComponent, StableId};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// A homing projectile: it flies `speed` a tick towards `target`, and strikes it for `amount` on
/// arrival, on behalf of `source`, with the roll the attack that fired it drew.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Projectile {
    source: StableId,
    target: StableId,
    speed: Num,
    amount: Num,
    roll: Num,
}

impl Projectile {
    /// `None` unless the speed is positive, the amount is not negative, and the roll is at least 0
    /// and less than 1.
    pub const fn new(
        source: StableId,
        target: StableId,
        speed: Num,
        amount: Num,
        roll: Num,
    ) -> Option<Projectile> {
        let bits = roll.to_bits();
        if speed.to_bits() <= 0 || amount.to_bits() < 0 || bits < 0 || bits >= Num::ONE.to_bits() {
            return None;
        }
        Some(Projectile {
            source,
            target,
            speed,
            amount,
            roll,
        })
    }

    pub const fn source(self) -> StableId {
        self.source
    }

    pub const fn target(self) -> StableId {
        self.target
    }

    pub const fn speed(self) -> Num {
        self.speed
    }

    pub const fn amount(self) -> Num {
        self.amount
    }

    pub const fn roll(self) -> Num {
        self.roll
    }
}

impl SimComponent for Projectile {
    const NAME: &'static str = "projectiles.projectile";
}

/// A snapshot is untrusted, so a projectile `new` refuses fails to decode.
impl<'de> Deserialize<'de> for Projectile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Projectile, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            source: StableId,
            target: StableId,
            speed: Num,
            amount: Num,
            roll: Num,
        }
        let fields = Fields::deserialize(deserializer)?;
        let Fields {
            source,
            target,
            speed,
            amount,
            roll,
        } = fields;
        Projectile::new(source, target, speed, amount, roll)
            .ok_or_else(|| D::Error::custom("projectile out of its limits"))
    }
}
