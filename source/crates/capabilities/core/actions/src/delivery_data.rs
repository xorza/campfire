use std::num::NonZeroU8;

use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_script::{DataSection, DataTable};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// An action's `delivery`: the unit type of its package it launches, a projectile type with
/// `count` of them spread evenly over `spread_deg` degrees around its aim, or an area type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryData {
    Projectile {
        unit_type: DeclaredName,
        count: NonZeroU8,
        spread_deg: Num,
    },
    Area {
        unit_type: DeclaredName,
    },
}

impl DeliveryData {
    /// The name of the unit type it launches, in its package.
    pub const fn unit_type(&self) -> &DeclaredName {
        match self {
            Self::Projectile { unit_type, .. } | Self::Area { unit_type } => unit_type,
        }
    }
}

/// Data is untrusted, so a delivery that names both a projectile and an area, or neither, an
/// area with a count or a spread, or a spread that is negative or past a full turn, fails to
/// read.
impl<'de> Deserialize<'de> for DeliveryData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            projectile: Option<DeclaredName>,
            area: Option<DeclaredName>,
            count: Option<NonZeroU8>,
            spread_deg: Option<Num>,
        }
        let fields = Fields::deserialize(deserializer)?;
        match (fields.projectile, fields.area) {
            (Some(unit_type), None) => {
                let spread_deg = match fields.spread_deg {
                    None => Num::ZERO,
                    Some(spread) if spread >= Num::ZERO && spread <= Num::int(360) => spread,
                    Some(_) => return Err(D::Error::custom("a spread from 0 to 360 degrees")),
                };
                Ok(Self::Projectile {
                    unit_type,
                    count: fields.count.unwrap_or(NonZeroU8::MIN),
                    spread_deg,
                })
            }
            (None, Some(unit_type)) if fields.count.is_none() && fields.spread_deg.is_none() => {
                Ok(Self::Area { unit_type })
            }
            (None, Some(_)) => Err(D::Error::custom("an area takes no count or spread_deg")),
            _ => Err(D::Error::custom("a delivery names a projectile or an area")),
        }
    }
}

impl DataSection for DeliveryData {
    const TABLE: DataTable = DataTable::Delivery;
}
