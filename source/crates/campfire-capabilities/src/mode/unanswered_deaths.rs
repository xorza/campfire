use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_sim::{SimResource, StableId};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::combat::deaths::DeathView;
use crate::scripts::pending_calls::PendingCalls;

/// The deaths whose `on_unit_died` has yet to run, in the order the units died. The Mode stage
/// adds its tick's deaths and runs the calls from the front; the deaths whose call found the mode
/// pool spent stay, and run first in a later tick's Mode stage. So a tick ends with only those.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct UnansweredDeaths {
    deaths: PendingCalls<Unanswered>,
    /// Each death's assisters, one run per death, in order.
    assisters: Vec<StableId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Unanswered {
    unit: StableId,
    killer: Option<StableId>,
    /// The length of its run of assisters.
    assisters: u32,
}

/// A death as `UnansweredDeaths` gives it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DeathCall<'a> {
    pub(crate) unit: StableId,
    pub(crate) killer: Option<StableId>,
    pub(crate) assisters: &'a [StableId],
}

impl UnansweredDeaths {
    pub(crate) const fn is_empty(&self) -> bool {
        self.deaths.is_empty()
    }

    /// Adds `deaths` at the end.
    pub(crate) fn extend<'a>(&mut self, deaths: impl IntoIterator<Item = DeathView<'a>>) {
        for death in deaths {
            self.assisters.extend_from_slice(death.assisters);
            self.deaths.extend([Unanswered {
                unit: death.fallen.unit,
                killer: death.killer,
                assisters: u32::try_from(death.assisters.len()).expect("assisters fit in u32"),
            }]);
        }
    }

    /// The deaths, first to last.
    pub(crate) fn iter(&self) -> impl Iterator<Item = DeathCall<'_>> {
        let mut start = 0;
        self.deaths.iter().map(move |death| {
            let end = start + death.assisters as usize;
            let assisters = &self.assisters[start..end];
            start = end;
            DeathCall {
                unit: death.unit,
                killer: death.killer,
                assisters,
            }
        })
    }

    /// Removes the first `count` deaths, whose calls ran.
    pub(crate) fn answered(&mut self, count: usize) {
        let runs = self.deaths.iter().take(count);
        let assisters: usize = runs.map(|death| death.assisters as usize).sum();
        self.deaths.answered(count);
        self.assisters.drain(..assisters);
    }
}

/// A snapshot is untrusted, so runs of assisters that do not cover the assisters exactly fail to
/// decode.
impl<'de> Deserialize<'de> for UnansweredDeaths {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<UnansweredDeaths, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            deaths: PendingCalls<Unanswered>,
            assisters: Vec<StableId>,
        }
        let Fields { deaths, assisters } = Fields::deserialize(deserializer)?;
        let mut runs = deaths.iter();
        let covered = runs.try_fold(0usize, |sum, death| {
            sum.checked_add(death.assisters as usize)
        });
        if covered != Some(assisters.len()) {
            return Err(D::Error::custom("runs that cover the assisters exactly"));
        }
        Ok(UnansweredDeaths { deaths, assisters })
    }
}

impl SimResource for UnansweredDeaths {
    const NAME: &'static str = "mode.unanswered_deaths";

    // Its decode keeps each death's assisters within the list, and each id may be gone.
    fn check(&self, _: &World) -> bool {
        true
    }
}
