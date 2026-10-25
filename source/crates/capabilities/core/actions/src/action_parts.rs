use campfire_script::{Filter, UnknownName};
use campfire_sim::MatchRate;
use campfire_values::{ModifierId, PackageIndex, ResourceId, TrackId};

use crate::action::{Aim, Passive};
use crate::action_data::ActionData;
use crate::action_names::ActionNames;
use crate::channel_pay::ChannelPay;
use crate::delivery::Delivery;
use crate::error::{ActionError, ActionField, ActionProblem};
use crate::rank_values::LoadedRanks;
use crate::targeting::Targeting;

/// What an action's data gives the pipeline, resolved against the match: its passive and its
/// hold, its aim, its fields at each rank, and how it delivers. Its kind's capability resolves
/// what the kind's section gives (04-capabilities/actions.md, Each kind owns its section).
#[derive(Debug)]
pub struct ActionParts {
    pub passive: Option<Passive>,
    pub hold: Option<ModifierId>,
    pub aim: Aim,
    pub when: Option<Filter>,
    pub ranks: LoadedRanks,
    pub delivery: Option<Delivery>,
    pub pays: Option<ChannelPay>,
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
        names: &ActionNames<'_>,
    ) -> Result<Self, ActionError> {
        let passive = data.keys.passive_modifier.as_ref().map(|name| {
            Ok::<_, UnknownName>(Passive {
                modifier: names.named::<ModifierId>((package, name.as_str()))?,
                while_ready: data.keys.passive_while_ready,
            })
        });
        let passive = passive.transpose()?;
        let aim = match &data.keys.targeting {
            Targeting::None => Aim::None,
            Targeting::Point => Aim::Point {
                clamp: data.keys.clamp_to_range,
            },
            Targeting::Direction => Aim::Direction,
            Targeting::Unit(filter) => Aim::Unit(names.filter(filter)?),
        };
        let when = data
            .keys
            .when
            .as_ref()
            .map(|when| names.filter(when.filter()));
        let when = when.transpose()?;
        let ranks = data.load_ranks(ranks, rate, |name| names.cost_target(name))?;
        let delivery = data.delivery.as_ref().map(|delivery| {
            let name = delivery.unit_type();
            let unit_type = names.unit_type(package, name)?;
            let aim = names
                .aim(unit_type)
                .ok_or_else(|| ActionProblem::Section(name.clone()))?;
            Ok::<_, ActionError>(Delivery {
                unit_type,
                capability: delivery.capability(),
                fan: delivery.fan(),
                aim,
            })
        });
        let delivery = delivery.transpose()?;
        let hold = data
            .keys
            .hold
            .as_ref()
            .map(|name| names.named::<ModifierId>((package, name.as_str())));
        let hold = hold.transpose()?;
        let pays = data
            .keys
            .channel
            .as_ref()
            .and_then(|channel| channel.pays.as_ref());
        let pays = pays.map(|pays| {
            let resource = names.named::<ResourceId>(pays.resource.as_str())?;
            let track = pays
                .track
                .as_ref()
                .map(|track| names.named::<TrackId>(track.as_str()));
            let pay = ChannelPay::new(resource, track.transpose()?, &pays.amount);
            Ok::<_, ActionError>(pay.ok_or(ActionProblem::Field(ActionField::Channel))?)
        });
        Ok(Self {
            passive,
            hold,
            aim,
            when,
            ranks,
            delivery,
            pays: pays.transpose()?,
        })
    }
}
