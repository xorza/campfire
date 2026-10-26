use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use campfire_script::{
    Ctx, Hook, HookDecl, ModeAnswer, ModeEvents, PendingCalls, ScriptList, ScriptRole, ScriptValue,
};
use campfire_sim::{EntityIndex, SimResource, StableId};
use campfire_units::{DeathView, Deaths};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::kept::Kept;

/// `on_unit_death(ctx, unit, killer, assisters)`: a unit died, its killer `()` when none or gone.
#[derive(Debug)]
pub struct OnUnitDeath;

impl HookDecl for OnUnitDeath {
    const NAME: &'static str = "on_unit_death";
    const ROLE: ScriptRole = ScriptRole::Mode;
    const PARAMS: &'static [&'static str] = &["ctx", "unit", "killer", "assisters"];
    type Args = (Ctx, ScriptValue, ScriptValue, ScriptList);
}

/// The deaths whose `on_unit_death` has yet to run, in the order the units died. The Mode stage
/// adds its tick's deaths and runs the calls from the front; the deaths whose call found the mode
/// budget spent stay, and run first in a later tick's Mode stage. So a tick ends with only those.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct UnansweredDeaths {
    deaths: PendingCalls<Unanswered>,
    /// Each death's assisters, one run per death, in order.
    assisters: Vec<StableId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Unanswered {
    id: StableId,
    killer: Option<StableId>,
    /// The length of its run of assisters.
    assisters: u32,
}

/// A death as `UnansweredDeaths` gives it.
#[derive(Debug, Clone, Copy)]
pub struct DeathCall<'a> {
    pub id: StableId,
    pub(crate) killer: Option<StableId>,
    pub(crate) assisters: &'a [StableId],
}

impl UnansweredDeaths {
    /// Adds `deaths` at the end.
    pub(crate) fn extend<'a>(&mut self, deaths: impl IntoIterator<Item = DeathView<'a>>) {
        for death in deaths {
            self.assisters.extend_from_slice(death.assisters);
            self.deaths.extend([Unanswered {
                id: death.fallen.id,
                killer: death.killer,
                assisters: u32::try_from(death.assisters.len()).expect("assisters fit in u32"),
            }]);
        }
    }

    /// The deaths, first to last.
    pub fn iter(&self) -> impl Iterator<Item = DeathCall<'_>> {
        let mut start = 0;
        self.deaths.iter().map(move |death| {
            let end = start + death.assisters as usize;
            let assisters = &self.assisters[start..end];
            start = end;
            DeathCall {
                id: death.id,
                killer: death.killer,
                assisters,
            }
        })
    }
}

/// A death runs `on_unit_death`, from the mode's budget, with the unit, its killer or `()`, and its
/// assisters: a killer gone by then is `()`, and an assister gone by then is left out; a death
/// whose unit is gone runs no call. A death that waits keeps its unit, dead, until its call runs.
impl ModeEvents for UnansweredDeaths {
    const HOOKS: &'static [Hook] = &[Hook::of::<OnUnitDeath>()];

    fn arrive(world: &mut World) {
        if world.resource::<Deaths>().is_empty() {
            return;
        }
        world.resource_scope(|world, mut deaths: Mut<'_, Self>| {
            deaths.extend(world.resource::<Deaths>().iter());
        });
    }

    fn len(&self) -> usize {
        self.deaths.len()
    }

    fn partial(&self) -> u8 {
        self.deaths.partial()
    }

    fn answer(&self, answer: &mut ModeAnswer<'_, '_>) -> usize {
        self.iter()
            .take_while(|death| {
                let args = |ctx: &Ctx| {
                    let view = ctx.view();
                    let unit = view.unit(death.id)?;
                    let killer = death.killer.and_then(|id| view.unit(id));
                    let killer = killer.map_or(ScriptValue::UNIT, ScriptValue::of);
                    let assisters = death.assisters.iter().filter_map(|&id| view.unit(id));
                    let assisters: ScriptList = assisters.map(ScriptValue::of).collect();
                    Some((ctx.clone(), ScriptValue::of(unit), killer, assisters))
                };
                answer
                    .call::<OnUnitDeath>(Some(death.id), args)
                    .is_continue()
            })
            .count()
    }

    fn answered(&mut self, count: usize, partial: u8) {
        let runs = self.deaths.iter().take(count);
        let assisters: usize = runs.map(|death| death.assisters as usize).sum();
        self.deaths.answered(count, partial);
        self.assisters.drain(..assisters);
    }

    fn append(&mut self, later: &mut Self) {
        self.deaths.append(&mut later.deaths);
        self.assisters.append(&mut later.assisters);
    }

    fn hold(&self, answered: usize, world: &mut World) {
        for (at, death) in self.iter().enumerate() {
            let Some(entity) = world.resource::<EntityIndex>().get(death.id) else {
                continue;
            };
            let mut unit = world.entity_mut(entity);
            if at < answered {
                unit.remove::<Kept>();
            } else {
                unit.insert(Kept);
            }
        }
    }
}

/// A snapshot is untrusted, so runs of assisters that do not cover the assisters exactly fail to
/// decode.
impl<'de> Deserialize<'de> for UnansweredDeaths {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
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
        Ok(Self { deaths, assisters })
    }
}

impl SimResource for UnansweredDeaths {
    const NAME: &'static str = "mode.unanswered_deaths";

    // Its decode keeps each death's assisters within the list, and each id may be gone.
    fn check(&self, _: &World) -> bool {
        true
    }
}
