use campfire_math::Num;
use campfire_script::{NameKind, UnknownName};
use campfire_sim::TickRate;
use campfire_values::{Distance, ModifierId, PackageIndex};

use crate::action::{Aim, Passive};
use crate::action_data::ActionData;
use crate::action_names::ActionNames;
use crate::cost_target::CostTarget;
use crate::delivery::{Delivery, DeliveryShape};
use crate::delivery_data::DeliveryData;
use crate::error::{ActionError, ActionProblem};
use crate::fan::Fan;
use crate::gather_spec::GatherSpec;
use crate::kind_data::{FromData, KindData};
use crate::kind_spec::KindSpec;
use crate::rank_values::{LoadedRanks, RankValues};
use crate::targeting::Targeting;
use crate::weapon::{Source, Weapon};

/// What an action's data gives, resolved against the match: its kind with what the kind needs,
/// its passive and its hold, its aim, its fields at each rank, and how it delivers.
#[derive(Debug)]
pub struct ActionParts {
    pub kind: KindSpec,
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
        rate: TickRate,
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
        let kind = match KindData::of(data)? {
            KindData::Cast => KindSpec::Cast,
            KindData::Use => KindSpec::Use,
            KindData::Attack {
                rate,
                damage,
                damage_kind,
            } => {
                let source = |from| match from {
                    FromData::Stat(stat) => Ok::<_, UnknownName>(Source::Stat(names.stat(stat)?)),
                    FromData::Own => Ok(Source::Own),
                };
                KindSpec::Attack(Weapon {
                    rate: source(rate)?,
                    damage: source(damage)?,
                    kind: names.damage_kind(damage_kind)?,
                })
            }
            KindData::Train { unit_type } => KindSpec::Train(names.unit_type(package, unit_type)?),
            KindData::Build { unit_type } => KindSpec::Build(names.unit_type(package, unit_type)?),
            KindData::Gather {
                resource,
                take,
                bounce,
            } => {
                let Some(CostTarget::Resource(resource)) = names.cost_target(resource) else {
                    return Err(UnknownName::new(NameKind::Resource, resource).into());
                };
                let bounce = bounce.map_or(Num::ZERO, Distance::get);
                KindSpec::Gather(GatherSpec {
                    resource,
                    take,
                    bounce,
                })
            }
        };
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
            kind,
            passive,
            hold,
            aim,
            ranks,
            delivery,
        })
    }
}
