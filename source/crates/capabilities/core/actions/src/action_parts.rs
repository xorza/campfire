use campfire_script::UnknownName;
use campfire_sim::MatchRate;
use campfire_values::{ModifierId, PackageIndex};

use crate::action::{Aim, Passive};
use crate::action_data::ActionData;
use crate::action_names::ActionNames;
use crate::delivery::{Delivery, DeliveryShape};
use crate::delivery_data::DeliveryData;
use crate::error::{ActionError, ActionProblem};
use crate::fan::Fan;
use crate::rank_values::{LoadedRanks, RankValues};
use crate::targeting::Targeting;

/// What an action's data gives the pipeline, resolved against the match: its passive and its
/// hold, its aim, its fields at each rank, and how it delivers. Its kind's capability resolves
/// what the kind's section gives (04-capabilities/actions.md, Each kind owns its section).
#[derive(Debug)]
pub struct ActionParts {
    pub passive: Option<Passive>,
    pub hold: Option<ModifierId>,
    pub aim: Aim,
    pub ranks: LoadedRanks,
    pub delivery: Option<Delivery>,
}

impl ActionParts {
    /// The parts of `data`, of `package`, at each of its `ranks` ranks, times in ticks at `rate`,
    /// its names resolved by `names`; an error when a time does not count in ticks, or for a name
    /// the match does not have.
    pub fn of(
        data: &ActionData,
        package: PackageIndex,
        ranks: u8,
        rate: MatchRate,
        names: &impl ActionNames,
    ) -> Result<Self, ActionError> {
        let passive = data.passive_modifier.as_ref().map(|name| {
            Ok::<_, UnknownName>(Passive {
                modifier: names.modifier(package, name)?,
                while_ready: data.passive_while_ready,
            })
        });
        let passive = passive.transpose()?;
        let aim = match &data.targeting {
            Targeting::None => Aim::None,
            Targeting::Point => Aim::Point {
                clamp: data.clamp_to_range,
            },
            Targeting::Direction => Aim::Direction,
            Targeting::Unit(filter) => Aim::Unit(names.filter(filter)?),
        };
        let ranks = RankValues::all(data, ranks, rate, |name| names.cost_target(name))?;
        let delivery = data.delivery.as_ref().map(|delivery| {
            let name = delivery.unit_type();
            let unit_type = names.unit_type(package, name)?;
            let shape = match *delivery {
                DeliveryData::Projectile {
                    count, spread_deg, ..
                } => DeliveryShape::Projectile {
                    fan: Fan { count, spread_deg },
                    flight: names
                        .flight(package, name)
                        .ok_or_else(|| ActionProblem::Section(name.clone()))?,
                },
                DeliveryData::Area { .. } => DeliveryShape::Area,
            };
            Ok::<_, ActionError>(Delivery { unit_type, shape })
        });
        let delivery = delivery.transpose()?;
        let hold = data.hold.as_ref().map(|name| names.modifier(package, name));
        let hold = hold.transpose()?;
        Ok(Self {
            passive,
            hold,
            aim,
            ranks,
            delivery,
        })
    }
}
