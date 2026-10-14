use campfire_combat::{DeathView, Fallen};
use campfire_common::{PlayerSlot, Tick};
use campfire_log::LogEvent;
use campfire_sim::StableId;
use campfire_values::Team;
use serde::Deserialize;
use tracing::{debug, info};

/// A unit of `team` died, dealt its last damage by `killer` if any; an avatar names the player
/// whose it was. `tick` is the sim tick of the end that logs it: on the server the tick the unit
/// died in, and on a client its own, which runs ahead of the server's, once it runs one. Each
/// end logs an avatar's death at `info`, any other at `debug`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) struct UnitDied {
    #[serde(default)]
    pub tick: Option<Tick>,
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
            tick: Some(tick),
            unit: death.fallen.id,
            team: death.fallen.team,
            owner: death.fallen.owner,
            killer: death.killer,
        }
    }

    /// The death of `fallen`, as a client learns it from the server's state at its own `tick`,
    /// with no killer.
    pub(crate) const fn learned(tick: Option<Tick>, fallen: Fallen) -> UnitDied {
        UnitDied {
            tick,
            unit: fallen.id,
            team: fallen.team,
            owner: fallen.owner,
            killer: None,
        }
    }
}

impl LogEvent for UnitDied {
    const MESSAGE: &'static str = "a unit died";

    fn log(&self) {
        let (tick, unit) = (self.tick.map(Tick::get), self.unit.get());
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
