use bevy_ecs::resource::Resource;
use campfire_script::{ScriptHost, ScriptId};
use campfire_sim::TickRate;
use serde::{Deserialize, Serialize};

use crate::abilities::ability_data::{AbilityData, Range, Targeting};
use crate::abilities::error::AbilityError;
use crate::abilities::frame::Frame;
use crate::units::hook::Hook;
use crate::units::param::Param;
use crate::units::ranked::Ranked;

/// The abilities a match loaded, times in ticks and scripts compiled. Package data, not state: a
/// restore loads it from the packages, as a new match does.
#[derive(Resource, Debug, Default)]
pub(crate) struct AbilityBook {
    abilities: Vec<Ability>,
}

/// An ability, by its place in the book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AbilityId(u32);

/// An ability as a match runs it.
#[derive(Debug)]
pub(crate) struct Ability {
    pub(crate) targeting: Targeting,
    pub(crate) range: Ranked<Range>,
    /// In ticks.
    pub(crate) cooldown: Ranked<u64>,
    pub(crate) cost: Ranked<u64>,
    /// In ticks.
    pub(crate) cast_time: Ranked<u64>,
    /// The script, when it defines `on_cast`: a script may serve only the ability's modifiers.
    pub(crate) on_cast: Option<ScriptId>,
    /// In the order of their names.
    pub(crate) params: Vec<Param>,
}

impl AbilityBook {
    /// Loads `data`; see `Abilities::load`. `frame` takes its param names.
    pub(crate) fn load(
        &mut self,
        host: &mut ScriptHost,
        frame: &mut Frame,
        data: &AbilityData,
        source: Option<&str>,
        rate: TickRate,
    ) -> Result<AbilityId, AbilityError> {
        let mut ranks = data.rank_counts();
        if let Some(first) = ranks.next()
            && ranks.any(|count| count != first)
        {
            return Err(AbilityError::RankCounts);
        }

        let script = match (&data.script, source) {
            (Some(_), Some(source)) => Some(host.compile(source).map_err(AbilityError::Script)?),
            (None, None) => None,
            _ => return Err(AbilityError::ScriptMismatch),
        };
        let on_cast = Hook::OnCast;
        let on_cast =
            script.filter(|&script| host.defines(script, on_cast.name(), on_cast.params()));
        let id = AbilityId(u32::try_from(self.abilities.len()).expect("abilities fit u32"));
        let cooldown = ticks(data.cooldown_ms.as_ref(), rate)?;
        let cast_time = ticks(data.cast_time_ms.as_ref(), rate)?;
        frame.add_param_names(data.params.keys().map(String::as_str));
        self.abilities.push(Ability {
            targeting: data.targeting,
            range: data.range.clone().unwrap_or(Ranked::One(Range::Global)),
            cooldown,
            cost: data.cost.clone().unwrap_or(Ranked::One(0)),
            cast_time,
            on_cast,
            params: data.params.values().cloned().collect(),
        });
        Ok(id)
    }

    pub(crate) fn get(&self, id: AbilityId) -> Option<&Ability> {
        self.abilities.get(id.index())
    }
}

impl AbilityId {
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Milliseconds, 0 when absent, in ticks at `rate`, rounded up.
fn ticks(ms: Option<&Ranked<u64>>, rate: TickRate) -> Result<Ranked<u64>, AbilityError> {
    let one = |ms: u64| rate.ticks(ms).ok_or(AbilityError::TimeTooLarge);
    Ok(match ms {
        None => Ranked::One(0),
        Some(Ranked::One(ms)) => Ranked::One(one(*ms)?),
        Some(Ranked::PerRank(ms)) => {
            Ranked::PerRank(ms.iter().map(|&ms| one(ms)).collect::<Result<_, _>>()?)
        }
    })
}
