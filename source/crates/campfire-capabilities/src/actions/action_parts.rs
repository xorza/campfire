use campfire_math::Num;
use campfire_sim::TickRate;

use crate::actions::action::Aim;
use crate::actions::action::Passive;
use crate::actions::action_data::ActionData;
use crate::actions::action_names::ActionNames;
use crate::actions::cost_target::CostTarget;
use crate::actions::delivery::Delivery;
use crate::actions::delivery::DeliveryShape;
use crate::actions::delivery_data::DeliveryData;
use crate::actions::error::{ActionError, ActionProblem};
use crate::actions::fan::Fan;
use crate::actions::gather_spec::GatherSpec;
use crate::actions::kind_data::KindData;
use crate::actions::kind_spec::KindSpec;
use crate::actions::rank_values::LoadedRanks;
use crate::actions::rank_values::RankValues;
use crate::actions::targeting::Targeting;
use crate::actions::weapon::Weapon;
use crate::scripts::error::UnknownName;
use crate::scripts::name_kind::NameKind;
use crate::units::modifier_id::ModifierId;
use crate::values::scalar::Scalar;

/// What an action's data gives, resolved against the match: its kind with what the kind needs,
/// its passive and its hold, its aim, its fields at each rank, and how it delivers.
#[derive(Debug)]
pub(crate) struct ActionParts {
    pub(crate) kind: KindSpec,
    pub(crate) passive: Option<Passive>,
    pub(crate) hold: Option<ModifierId>,
    pub(crate) aim: Aim,
    pub(crate) ranks: LoadedRanks,
    pub(crate) delivery: Option<Delivery>,
}

impl ActionParts {
    /// The parts of `data`, of `package`, at each of its `ranks` ranks, times in ticks at `rate`,
    /// its names resolved by `names`; an error when a time does not count in ticks, or for a name
    /// the match does not have.
    pub(crate) fn of(
        data: &ActionData,
        package: u16,
        ranks: u8,
        rate: TickRate,
        names: &impl ActionNames,
    ) -> Result<ActionParts, ActionError> {
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
        let kind = match data.kind_data().map_err(ActionProblem::KindField)? {
            KindData::Cast => KindSpec::Cast,
            KindData::Attack {
                rate,
                damage,
                damage_kind,
            } => KindSpec::Attack(Weapon {
                rate: names.stat(rate)?,
                damage: names.stat(damage)?,
                kind: names.damage_kind(damage_kind)?,
            }),
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
                let bounce = bounce.map_or(Some(Num::ZERO), Scalar::to_num);
                let bounce = bounce
                    .filter(|&bounce| bounce >= Num::ZERO)
                    .ok_or(ActionProblem::Bounce)?;
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
                    homes: names
                        .homes(package, name)
                        .ok_or_else(|| ActionProblem::Section(name.clone()))?,
                },
                DeliveryData::Area { .. } => DeliveryShape::Area,
            };
            Ok::<_, ActionError>(Delivery { unit_type, shape })
        });
        let delivery = delivery.transpose()?;
        let hold = data.hold.as_ref().map(|name| names.modifier(package, name));
        let hold = hold.transpose()?;
        Ok(ActionParts {
            kind,
            passive,
            hold,
            aim,
            ranks,
            delivery,
        })
    }
}
