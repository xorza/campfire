use std::num::NonZeroU8;

use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::values::scalar::Scalar;

/// An action's `delivery`: the projectile type of its package it launches, `count` of them,
/// spread evenly over `spread_deg` degrees around its aim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryData {
    pub projectile: String,
    pub count: NonZeroU8,
    pub spread_deg: Num,
}

/// Data is untrusted, so a spread that is negative or past a full turn fails to read.
impl<'de> Deserialize<'de> for DeliveryData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<DeliveryData, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            projectile: String,
            count: Option<NonZeroU8>,
            spread_deg: Option<Scalar>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let spread_deg = match fields.spread_deg {
            None => Num::ZERO,
            Some(spread) => spread
                .to_num()
                .filter(|&spread| {
                    spread >= Num::ZERO && spread <= Num::from_bits(360 << Num::FRAC_BITS)
                })
                .ok_or_else(|| D::Error::custom("a spread from 0 to 360 degrees"))?,
        };
        Ok(DeliveryData {
            projectile: fields.projectile,
            count: fields.count.unwrap_or(NonZeroU8::MIN),
            spread_deg,
        })
    }
}
