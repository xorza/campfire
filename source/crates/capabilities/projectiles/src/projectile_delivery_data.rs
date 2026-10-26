use std::num::NonZeroU8;
use std::sync::Arc;

use campfire_actions::{DeliverySection, DeliverySections, Fan};
use campfire_common::DeclaredName;
use campfire_declarations::{Listed, ListedSet};
use campfire_math::Num;
use campfire_script::{ApiBuilder, DataSection, DataTable};
use campfire_script::{ApiVersion, Status};
use campfire_sim::Capability;
use serde::de::Error;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An action's delivery of projectiles, `delivery = { projectile, count, spread_deg }`: the
/// projectile type of its package it launches, `count` of them spread evenly over `spread_deg`
/// degrees around its aim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectileDeliveryData {
    pub projectile: DeclaredName,
    pub count: NonZeroU8,
    pub spread_deg: Num,
}

impl ProjectileDeliveryData {
    /// One projectile of `projectile`, at the aim.
    pub const fn one(projectile: DeclaredName) -> Self {
        Self {
            projectile,
            count: NonZeroU8::MIN,
            spread_deg: Num::ZERO,
        }
    }
}

/// Data is untrusted, so a spread that is negative or past a full turn fails to read.
impl<'de> Deserialize<'de> for ProjectileDeliveryData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            projectile: DeclaredName,
            count: Option<NonZeroU8>,
            spread_deg: Option<Num>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let spread_deg = match fields.spread_deg {
            None => Num::ZERO,
            Some(spread) if (Num::ZERO..=Num::int(360)).contains(&spread) => spread,
            Some(_) => return Err(D::Error::custom("a spread from 0 to 360 degrees")),
        };
        Ok(Self {
            projectile: fields.projectile,
            count: fields.count.unwrap_or(NonZeroU8::MIN),
            spread_deg,
        })
    }
}

/// As data writes it, what `Deserialize` reads: its count unless 1 and its spread unless 0.
impl Serialize for ProjectileDeliveryData {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("projectile", &self.projectile)?;
        if self.count != NonZeroU8::MIN {
            map.serialize_entry("count", &self.count)?;
        }
        if self.spread_deg != Num::ZERO {
            map.serialize_entry("spread_deg", &self.spread_deg)?;
        }
        map.end()
    }
}

impl DeliverySection for ProjectileDeliveryData {
    fn capability(&self) -> Capability {
        Capability::Projectiles
    }

    fn unit_type(&self) -> &DeclaredName {
        &self.projectile
    }

    fn fan(&self) -> Fan {
        Fan::new(self.count, self.spread_deg)
    }
}

impl DataSection for ProjectileDeliveryData {
    const TABLE: DataTable = DeliverySections::TABLE;
}

impl Listed for ProjectileDeliveryData {
    type Set = DeliverySections;

    const KEY: &'static str = "projectile";

    const STATUS: Status = Status::Runs(ApiVersion::FIRST);

    fn into_listed(self) -> Arc<dyn DeliverySection> {
        Arc::new(self)
    }

    fn api(_: &mut ApiBuilder<'_>) {}
}
