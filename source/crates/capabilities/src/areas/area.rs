use bevy_ecs::component::Component;
use campfire_math::Tick;
use campfire_sim::{SimComponent, StableId};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::deliveries::delivering::Delivering;

/// An area unit on the ground: the delivery it is, whose `on_hit` and `on_end` it runs, the unit
/// its action aimed at, if one, the tick it triggers, `None` once it did, and the tick it ends,
/// never before its trigger.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Area {
    by: Delivering,
    aimed: Option<StableId>,
    triggers_at: Option<Tick>,
    ends_at: Tick,
}

impl Area {
    /// `None` when it would trigger after it ends.
    pub(crate) fn new(
        by: Delivering,
        aimed: Option<StableId>,
        triggers_at: Option<Tick>,
        ends_at: Tick,
    ) -> Option<Area> {
        triggers_at.is_none_or(|at| at <= ends_at).then_some(Area {
            by,
            aimed,
            triggers_at,
            ends_at,
        })
    }

    pub(crate) const fn by(&self) -> Delivering {
        self.by
    }

    pub(crate) const fn aimed(&self) -> Option<StableId> {
        self.aimed
    }

    pub(crate) const fn triggers_at(&self) -> Option<Tick> {
        self.triggers_at
    }

    pub(crate) const fn ends_at(&self) -> Tick {
        self.ends_at
    }

    /// Counts its trigger as done.
    pub(crate) const fn trigger(&mut self) {
        self.triggers_at = None;
    }
}

impl SimComponent for Area {
    const NAME: &'static str = "areas.area";
}

/// A snapshot is untrusted, so an area that would trigger after it ends fails to decode.
impl<'de> Deserialize<'de> for Area {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Area, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            by: Delivering,
            aimed: Option<StableId>,
            triggers_at: Option<Tick>,
            ends_at: Tick,
        }
        let fields = Fields::deserialize(deserializer)?;
        Area::new(fields.by, fields.aimed, fields.triggers_at, fields.ends_at)
            .ok_or_else(|| D::Error::custom("an area that triggers after it ends"))
    }
}
