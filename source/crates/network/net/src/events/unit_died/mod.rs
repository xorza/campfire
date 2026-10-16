use campfire_combat::{DeathView, Fallen};
use campfire_common::{PlayerSlot, Tick};
use campfire_log::LogEvent;
use campfire_sim::StableId;
use campfire_values::{NameId, Team};
use serde::Deserialize;
use tracing::{debug, info};

/// A unit of `team`, owned by `owner` if any, died, dealt its last damage by `killer` if any;
/// `avatar` says it was its owner's avatar. `tick` is the sim tick of the end that logs it: on the
/// server the tick the unit died in, and on a client its own, which runs ahead of the server's,
/// once it runs one. Each end logs an avatar's death at `info`, any other at `debug`.
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
    #[serde(default)]
    pub avatar: bool,
}

impl UnitDied {
    /// The death `death` of `tick`, as the server logs it.
    pub(crate) const fn of(tick: Tick, death: &DeathView<'_>) -> Self {
        Self {
            tick: Some(tick),
            unit: death.fallen.id,
            team: death.fallen.team,
            owner: death.fallen.owner,
            killer: death.killer,
            avatar: death.fallen.avatar,
        }
    }

    /// The death of `fallen`, as a client learns it from the server's state at its own `tick`,
    /// with no killer.
    pub(crate) const fn learned(tick: Option<Tick>, fallen: Fallen) -> Self {
        Self {
            tick,
            unit: fallen.id,
            team: fallen.team,
            owner: fallen.owner,
            killer: None,
            avatar: fallen.avatar,
        }
    }
}

impl LogEvent for UnitDied {
    const MESSAGE: &'static str = "a unit died";

    fn log(&self) {
        let (tick, unit) = (self.tick.map(Tick::get), self.unit.get());
        let team = self.team.map(Team::index);
        let killer = self.killer.map(StableId::get);
        let owner = self.owner.map(PlayerSlot::get);
        if self.avatar {
            info!(
                tick,
                unit,
                team,
                owner,
                killer,
                avatar = true,
                "{}",
                Self::MESSAGE
            );
        } else {
            debug!(tick, unit, team, owner, killer, "{}", Self::MESSAGE);
        }
    }
}

#[cfg(test)]
mod tests;
