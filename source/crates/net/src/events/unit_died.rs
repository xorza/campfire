use campfire_capabilities::{DeathView, Team};
use campfire_log::LogEvent;
use campfire_math::PlayerSlot;
use campfire_sim::{StableId, Tick};
use serde::Deserialize;
use tracing::{debug, info};

/// A unit of `team` died in `tick`, dealt its last damage by `killer` if any; an avatar names the
/// player whose it was. The server logs an avatar's death at `info`, any other at `debug`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct UnitDied {
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
mod tests {
    use campfire_capabilities::Fallen;
    use campfire_log::internals::round_trip;
    use campfire_sim::IdAllocator;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        let mut ids = IdAllocator::default();
        let [killer, unit] = [ids.allocate(), ids.allocate()];
        let avatar = UnitDied {
            tick: Tick::new(7),
            unit,
            team: Some(Team::new(1)),
            owner: Some(PlayerSlot::new(0)),
            killer: Some(killer),
        };
        round_trip(&avatar);
        // A unit with no team, owner or killer leaves those fields out.
        round_trip(&UnitDied {
            team: None,
            owner: None,
            killer: None,
            ..avatar
        });
    }

    #[test]
    fn a_death_logs_its_unit_as_it_fell_and_its_killer() {
        let mut ids = IdAllocator::default();
        let [killer, unit] = [ids.allocate(), ids.allocate()];
        let fallen = Fallen {
            unit,
            team: Some(Team::new(1)),
            owner: Some(PlayerSlot::new(0)),
        };
        let death = DeathView {
            fallen,
            killer: Some(killer),
            assisters: &[],
        };
        assert_eq!(
            UnitDied::of(Tick::new(7), &death),
            UnitDied {
                tick: Tick::new(7),
                unit,
                team: Some(Team::new(1)),
                owner: Some(PlayerSlot::new(0)),
                killer: Some(killer),
            }
        );
    }
}
