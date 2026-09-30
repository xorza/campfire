use std::ops::Range;

use bevy_ecs::resource::Resource;
use campfire_sim::StableId;

/// The units that died in this tick, in the order they died, each with its killer and the units
/// that assisted: what the mode receives in `on_unit_died`. Not state: it empties before each
/// tick's damage, and the Mode stage of the same tick reads it.
#[derive(Resource, Debug, Default)]
pub(crate) struct Deaths {
    deaths: Vec<Death>,
    /// Each death's assisters, one run per death.
    assisters: Vec<StableId>,
}

#[derive(Debug, Clone)]
struct Death {
    unit: StableId,
    killer: Option<StableId>,
    assisters: Range<usize>,
}

/// A death as `Deaths` gives it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DeathView<'a> {
    pub(crate) unit: StableId,
    pub(crate) killer: Option<StableId>,
    pub(crate) assisters: &'a [StableId],
}

impl Deaths {
    pub(crate) fn clear(&mut self) {
        self.deaths.clear();
        self.assisters.clear();
    }

    /// Records that `unit` died, dealt its last damage by `killer` if any, with `assisters`.
    pub(crate) fn push(
        &mut self,
        unit: StableId,
        killer: Option<StableId>,
        assisters: impl IntoIterator<Item = StableId>,
    ) {
        let start = self.assisters.len();
        self.assisters.extend(assisters);
        self.deaths.push(Death {
            unit,
            killer,
            assisters: start..self.assisters.len(),
        });
    }

    pub(crate) fn contains(&self, unit: StableId) -> bool {
        self.deaths.iter().any(|death| death.unit == unit)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.deaths.is_empty()
    }

    /// The deaths, in the order the units died.
    pub(crate) fn iter(&self) -> impl Iterator<Item = DeathView<'_>> {
        self.deaths.iter().map(|death| DeathView {
            unit: death.unit,
            killer: death.killer,
            assisters: &self.assisters[death.assisters.clone()],
        })
    }
}
