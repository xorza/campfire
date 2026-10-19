use campfire_common::EnumAll;

use crate::action_data::ActionData;
use crate::kind_takes::{KindTakes, RangeTakes, Takes};

/// A field of an action's data that the pipeline runs for every kind that takes it, beside the
/// name, the kind and the targeting every action gives, and the kind's own section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum PipelineField {
    Script,
    Range,
    CooldownMs,
    Cost,
    WindupMs,
    ClampToRange,
    Toggle,
    Channel,
    Hold,
    Charges,
    Charging,
    PassiveModifier,
    PassiveWhileReady,
    Delivery,
    Params,
    OnResolve,
    OnHit,
    OnEnd,
}

impl PipelineField {
    /// The field's name, as data writes it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Script => "script",
            Self::Range => "range",
            Self::CooldownMs => "cooldown_ms",
            Self::Cost => "cost",
            Self::WindupMs => "windup_ms",
            Self::ClampToRange => "clamp_to_range",
            Self::Toggle => "toggle",
            Self::Channel => "channel",
            Self::Hold => "hold",
            Self::Charges => "charges",
            Self::Charging => "charging",
            Self::PassiveModifier => "passive_modifier",
            Self::PassiveWhileReady => "passive_while_ready",
            Self::Delivery => "delivery",
            Self::Params => "params",
            Self::OnResolve => "on_resolve",
            Self::OnHit => "on_hit",
            Self::OnEnd => "on_end",
        }
    }

    /// Whether a kind that takes `takes` takes it.
    const fn taken(self, takes: KindTakes) -> Takes {
        match self {
            Self::Script => takes.script,
            Self::Range => match takes.range {
                RangeTakes::Never => Takes::Never,
                RangeTakes::May => Takes::May,
                RangeTakes::Meters => Takes::Must,
            },
            Self::CooldownMs => takes.cooldown,
            Self::Cost => takes.cost,
            Self::WindupMs => takes.windup,
            Self::ClampToRange => takes.clamp_to_range,
            Self::Toggle => takes.toggle,
            Self::Channel => takes.channel,
            Self::Hold => takes.hold,
            Self::Charges => takes.charges,
            Self::Charging => takes.charging,
            Self::PassiveModifier | Self::PassiveWhileReady => takes.passive,
            Self::Delivery => takes.delivery,
            Self::Params => takes.params,
            Self::OnResolve => takes.on_resolve,
            Self::OnHit => takes.on_hit,
            Self::OnEnd => takes.on_end,
        }
    }

    /// Whether `data` gives it: a value, a list or a table that is not empty, or a flag that is
    /// on.
    fn is_given(self, data: &ActionData) -> bool {
        match self {
            Self::Script => data.script.is_some(),
            Self::Range => data.range.is_some(),
            Self::CooldownMs => data.cooldown_ms.is_some(),
            Self::Cost => !data.cost.is_empty(),
            Self::WindupMs => data.windup_ms.is_some(),
            Self::ClampToRange => data.clamp_to_range,
            Self::Toggle => data.toggle.is_some(),
            Self::Channel => data.channel.is_some(),
            Self::Hold => data.hold.is_some(),
            Self::Charges => data.charges.is_some(),
            Self::Charging => data.charging.is_some(),
            Self::PassiveModifier => data.passive_modifier.is_some(),
            Self::PassiveWhileReady => data.passive_while_ready,
            Self::Delivery => data.delivery.is_some(),
            Self::Params => !data.params.is_empty(),
            Self::OnResolve => !data.on_resolve.is_empty(),
            Self::OnHit => !data.on_hit.is_empty(),
            Self::OnEnd => !data.on_end.is_empty(),
        }
    }

    /// The first field `data` gives that its kind never takes, or does not give that its kind
    /// always takes.
    pub fn misused(data: &ActionData) -> Option<Self> {
        let takes = data.section.takes();
        Self::ALL
            .into_iter()
            .find(|field| match field.taken(takes) {
                Takes::Never => field.is_given(data),
                Takes::May => false,
                Takes::Must => !field.is_given(data),
            })
    }
}
