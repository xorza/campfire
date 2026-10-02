use campfire_sim::TickRate;

use crate::actions::action::Aim;
use crate::actions::action::Passive;
use crate::actions::action_data::{ActionData, Targeting};
use crate::actions::action_kind::ActionKind;
use crate::actions::action_names::ActionNames;
use crate::actions::delivery::Delivery;
use crate::actions::delivery::DeliveryShape;
use crate::actions::delivery_data::DeliveryData;
use crate::actions::error::ActionError;
use crate::actions::fan::Fan;
use crate::actions::kind_spec::KindSpec;
use crate::actions::rank_values::LoadedRanks;
use crate::actions::rank_values::RankValues;
use crate::actions::weapon::Weapon;

/// What an action's data gives, resolved against the match: its kind with what the kind needs,
/// its passive, its aim, its fields at each rank, and how it delivers.
#[derive(Debug)]
pub(crate) struct ActionParts {
    pub(crate) kind: KindSpec,
    pub(crate) passive: Option<Passive>,
    pub(crate) aim: Aim,
    pub(crate) ranks: LoadedRanks,
    pub(crate) delivery: Option<Delivery>,
}

impl ActionParts {
    /// The parts of `data`, of `package`, at each of its `ranks` ranks, times in ticks at `rate`,
    /// its names resolved by `names`; an error when a time does not count in ticks.
    pub(crate) fn of(
        data: &ActionData,
        package: u16,
        ranks: u8,
        rate: TickRate,
        names: &impl ActionNames,
    ) -> Result<ActionParts, ActionError> {
        let passive = data.passive_modifier.as_ref().map(|name| Passive {
            modifier: names.modifier(package, name),
            while_ready: data.passive_while_ready,
        });
        let aim = match &data.targeting {
            Targeting::None => Aim::None,
            Targeting::Point => Aim::Point,
            Targeting::Direction => Aim::Direction,
            Targeting::Unit(filter) => Aim::Unit(names.filter(filter)),
        };
        let ranks = RankValues::all(data, ranks, rate, |name| names.cost_target(name))?;
        let kind = match data.kind {
            ActionKind::Cast => KindSpec::Cast,
            ActionKind::Attack => {
                let checked = "the load checked an attack's weapon fields";
                KindSpec::Attack(Weapon {
                    rate: names.stat(data.rate.as_ref().expect(checked)),
                    damage: names.stat(data.damage.as_ref().expect(checked)),
                    kind: names.damage_kind(data.damage_kind.as_ref().expect(checked)),
                })
            }
            ActionKind::Train => {
                let unit = data
                    .unit_type
                    .as_ref()
                    .expect("the load checked a train's unit");
                KindSpec::Train(names.unit_type(package, unit))
            }
            kind => panic!("the load runs no {kind:?}"),
        };
        let delivery = data.delivery.as_ref().map(|delivery| {
            let unit_type = names.unit_type(package, delivery.unit_type());
            let shape = match *delivery {
                DeliveryData::Projectile {
                    count, spread_deg, ..
                } => DeliveryShape::Projectile {
                    fan: Fan { count, spread_deg },
                    homes: names.homes(package, delivery.unit_type()),
                },
                DeliveryData::Area { .. } => DeliveryShape::Area,
            };
            Delivery { unit_type, shape }
        });
        Ok(ActionParts {
            kind,
            passive,
            aim,
            ranks,
            delivery,
        })
    }
}
