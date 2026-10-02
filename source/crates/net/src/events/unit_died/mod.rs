use campfire_capabilities::{DeathView, Team};
use campfire_log::LogEvent;
use campfire_math::{PlayerSlot, Tick};
use campfire_sim::StableId;
use serde::Deserialize;
use tracing::{debug, info};

/// A unit of `team` died in `tick`, dealt its last damage by `killer` if any; an avatar names the
/// player whose it was. The server logs an avatar's death at `info`, any other at `debug`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) struct UnitDied {
    pub tick: Tick,
    pub unit: StableId,
    #[serde(default)]
    pub team: Option<Team>,
    #[serde(default)]
    pub owner: Option<PlayerSlot>,
    #[serde(default)]
    pub killer: Option<StableId>,
}

impl UnitDied {
    /// The death `death` of `tick`, as the server logs it.
    pub(crate) const fn of(tick: Tick, death: &DeathView<'_>) -> UnitDied {
        UnitDied {
            tick,
            unit: death.fallen.unit,
            team: death.fallen.team,
            owner: death.fallen.owner,
            killer: death.killer,
        }
    }
}

impl LogEvent for UnitDied {
    const MESSAGE: &'static str = "a unit died";

    fn log(&self) {
        let (tick, unit) = (self.tick.get(), self.unit.get());
        let team = self.team.map(Team::index);
        let killer = self.killer.map(StableId::get);
        if let Some(owner) = self.owner {
            let owner = owner.get();
            info!(tick, unit, team, owner, killer, "{}", Self::MESSAGE);
        } else {
            debug!(tick, unit, team, killer, "{}", Self::MESSAGE);
        }
    }
}

#[cfg(test)]
mod tests;
