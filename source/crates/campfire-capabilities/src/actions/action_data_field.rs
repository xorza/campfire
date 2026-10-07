use campfire_sim::Capability;

use crate::actions::action_data::ActionData;
use crate::actions::action_kind::ActionKind;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::script_api::status::Status;

/// A field of an action's data, `[actions.<id>]`: the one table of which kinds of action take
/// each field, which need it, which capability runs it, and whether the release runs it yet.
/// The load check and the script API reference both read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionDataField {
    Kind,
    Script,
    Targeting,
    Range,
    CooldownMs,
    Cost,
    WindupMs,
    ClampToRange,
    Toggle,
    Channel,
    Hold,
    Charges,
    Charge,
    PassiveModifier,
    PassiveWhileReady,
    Delivery,
    Rate,
    Damage,
    DamageKind,
    UnitType,
    Requires,
    Construct,
    StartLife,
    CancelRefund,
    Placement,
    Resource,
    Take,
    Bounce,
    Params,
    OnResolve,
    OnHit,
    OnEnd,
}

/// What a kind of action does with a field: takes it or not, needs it, or refuses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldUse {
    Takes,
    Needs,
    Refuses,
}

/// The kinds the release runs, in the order of `FieldRule::uses`.
const KINDS: [ActionKind; 5] = [
    ActionKind::Cast,
    ActionKind::Attack,
    ActionKind::Train,
    ActionKind::Build,
    ActionKind::Gather,
];

/// A field's row of the table: its name as data writes it, the capability that runs it, none for
/// the core's, whether the release runs it, and its use by each kind the release runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FieldRule {
    name: &'static str,
    capability: Option<Capability>,
    runs: bool,
    uses: [FieldUse; KINDS.len()],
}

impl FieldRule {
    const fn new(
        name: &'static str,
        capability: Option<Capability>,
        runs: bool,
        uses: [FieldUse; KINDS.len()],
    ) -> FieldRule {
        FieldRule {
            name,
            capability,
            runs,
            uses,
        }
    }
}

impl ActionDataField {
    /// The fields `capability` runs, none for the action pipeline's, each by its name with its
    /// status in the script API.
    pub(crate) fn of(
        capability: Option<Capability>,
    ) -> impl Iterator<Item = (&'static str, Status)> {
        ActionDataField::ALL
            .into_iter()
            .filter(move |field| field.capability() == capability)
            .map(|field| {
                let status = if field.runs() {
                    Status::Runs(ApiVersion::FIRST)
                } else {
                    Status::Planned
                };
                (field.name(), status)
            })
    }

    pub const ALL: [ActionDataField; 32] = [
        ActionDataField::Kind,
        ActionDataField::Script,
        ActionDataField::Targeting,
        ActionDataField::Range,
        ActionDataField::CooldownMs,
        ActionDataField::Cost,
        ActionDataField::WindupMs,
        ActionDataField::ClampToRange,
        ActionDataField::Toggle,
        ActionDataField::Channel,
        ActionDataField::Hold,
        ActionDataField::Charges,
        ActionDataField::Charge,
        ActionDataField::PassiveModifier,
        ActionDataField::PassiveWhileReady,
        ActionDataField::Delivery,
        ActionDataField::Rate,
        ActionDataField::Damage,
        ActionDataField::DamageKind,
        ActionDataField::UnitType,
        ActionDataField::Requires,
        ActionDataField::Construct,
        ActionDataField::StartLife,
        ActionDataField::CancelRefund,
        ActionDataField::Placement,
        ActionDataField::Resource,
        ActionDataField::Take,
        ActionDataField::Bounce,
        ActionDataField::Params,
        ActionDataField::OnResolve,
        ActionDataField::OnHit,
        ActionDataField::OnEnd,
    ];

    #[expect(clippy::too_many_lines, reason = "a match of one row for each field")]
    const fn rule(self) -> FieldRule {
        use Capability::{Abilities, Combat, Production, Projectiles};
        use FieldUse::{Needs, Refuses, Takes};
        let (name, capability, runs, uses) = match self {
            ActionDataField::Kind => ("kind", None, true, [Takes, Takes, Takes, Takes, Takes]),
            ActionDataField::Script => (
                "script",
                Some(Abilities),
                true,
                [Takes, Refuses, Refuses, Refuses, Refuses],
            ),
            ActionDataField::Targeting => {
                ("targeting", None, true, [Takes, Takes, Takes, Takes, Takes])
            }
            ActionDataField::Range => ("range", None, true, [Takes, Needs, Refuses, Needs, Needs]),
            ActionDataField::CooldownMs => (
                "cooldown_ms",
                Some(Abilities),
                true,
                [Takes, Refuses, Takes, Takes, Refuses],
            ),
            ActionDataField::Cost => ("cost", None, true, [Takes, Takes, Takes, Takes, Refuses]),
            ActionDataField::WindupMs => {
                ("windup_ms", None, true, [Takes, Takes, Takes, Needs, Needs])
            }
            ActionDataField::ClampToRange => (
                "clamp_to_range",
                Some(Abilities),
                true,
                [Takes, Refuses, Refuses, Refuses, Refuses],
            ),
            ActionDataField::Toggle => (
                "toggle",
                Some(Abilities),
                true,
                [Takes, Refuses, Refuses, Refuses, Refuses],
            ),
            ActionDataField::Channel => (
                "channel",
                Some(Abilities),
                true,
                [Takes, Refuses, Refuses, Refuses, Refuses],
            ),
            ActionDataField::Hold => (
                "hold",
                Some(Abilities),
                true,
                [Takes, Refuses, Refuses, Refuses, Refuses],
            ),
            ActionDataField::Charges => (
                "charges",
                Some(Abilities),
                true,
                [Takes, Refuses, Refuses, Refuses, Refuses],
            ),
            ActionDataField::Charge => (
                "charge",
                Some(Abilities),
                true,
                [Takes, Refuses, Refuses, Refuses, Refuses],
            ),
            ActionDataField::PassiveModifier => (
                "passive_modifier",
                None,
                true,
                [Takes, Takes, Takes, Takes, Takes],
            ),
            ActionDataField::PassiveWhileReady => (
                "passive_while_ready",
                None,
                true,
                [Takes, Takes, Takes, Takes, Takes],
            ),
            ActionDataField::Delivery => (
                "delivery",
                Some(Projectiles),
                true,
                [Takes, Takes, Refuses, Refuses, Refuses],
            ),
            ActionDataField::Rate => (
                "rate",
                Some(Combat),
                true,
                [Refuses, Needs, Refuses, Refuses, Refuses],
            ),
            ActionDataField::Damage => (
                "damage",
                Some(Combat),
                true,
                [Refuses, Needs, Refuses, Refuses, Refuses],
            ),
            ActionDataField::DamageKind => (
                "damage_kind",
                Some(Combat),
                true,
                [Refuses, Needs, Refuses, Refuses, Refuses],
            ),
            ActionDataField::UnitType => (
                "unit_type",
                Some(Production),
                true,
                [Refuses, Refuses, Needs, Needs, Refuses],
            ),
            ActionDataField::Requires => (
                "requires",
                Some(Production),
                true,
                [Refuses, Refuses, Takes, Takes, Refuses],
            ),
            ActionDataField::Construct => (
                "construct",
                Some(Production),
                true,
                [Refuses, Refuses, Refuses, Needs, Refuses],
            ),
            ActionDataField::StartLife => (
                "start_life",
                Some(Production),
                true,
                [Refuses, Refuses, Refuses, Takes, Refuses],
            ),
            ActionDataField::CancelRefund => (
                "cancel_refund",
                Some(Production),
                true,
                [Refuses, Refuses, Refuses, Takes, Refuses],
            ),
            ActionDataField::Placement => (
                "placement",
                Some(Production),
                true,
                [Refuses, Refuses, Refuses, Takes, Refuses],
            ),
            ActionDataField::Resource => (
                "resource",
                Some(Production),
                true,
                [Refuses, Refuses, Refuses, Refuses, Needs],
            ),
            ActionDataField::Take => (
                "take",
                Some(Production),
                true,
                [Refuses, Refuses, Refuses, Refuses, Needs],
            ),
            ActionDataField::Bounce => (
                "bounce",
                Some(Production),
                true,
                [Refuses, Refuses, Refuses, Refuses, Takes],
            ),
            ActionDataField::Params => (
                "params",
                None,
                true,
                [Takes, Takes, Refuses, Refuses, Refuses],
            ),
            ActionDataField::OnResolve => (
                "on_resolve",
                None,
                true,
                [Takes, Refuses, Refuses, Refuses, Refuses],
            ),
            ActionDataField::OnHit => (
                "on_hit",
                None,
                true,
                [Takes, Takes, Refuses, Refuses, Refuses],
            ),
            ActionDataField::OnEnd => (
                "on_end",
                None,
                true,
                [Takes, Refuses, Refuses, Refuses, Refuses],
            ),
        };
        FieldRule::new(name, capability, runs, uses)
    }

    /// The field's name, as data writes it.
    pub const fn name(self) -> &'static str {
        self.rule().name
    }

    /// The capability that runs it; `None` for a field of the core.
    pub const fn capability(self) -> Option<Capability> {
        self.rule().capability
    }

    /// Whether the release runs it, beside loading it.
    pub const fn runs(self) -> bool {
        self.rule().runs
    }

    /// What an action of `kind`, one the release runs, does with it.
    pub(crate) fn use_by(self, kind: ActionKind) -> FieldUse {
        let at = KINDS
            .iter()
            .position(|&run| run == kind)
            .expect("a kind the release runs");
        self.rule().uses[at]
    }

    /// Whether `data` gives it: a value, a list or a table that is not empty, or a flag that is
    /// on. A field every action gives, its kind and its targeting, is always given.
    pub fn given(self, data: &ActionData) -> bool {
        match self {
            ActionDataField::Kind | ActionDataField::Targeting => true,
            ActionDataField::Script => data.script.is_some(),
            ActionDataField::Range => data.range.is_some(),
            ActionDataField::CooldownMs => data.cooldown_ms.is_some(),
            ActionDataField::Cost => !data.cost.is_empty(),
            ActionDataField::WindupMs => data.windup_ms.is_some(),
            ActionDataField::ClampToRange => data.clamp_to_range,
            ActionDataField::Toggle => data.toggle.is_some(),
            ActionDataField::Channel => data.channel.is_some(),
            ActionDataField::Hold => data.hold.is_some(),
            ActionDataField::Charges => data.charges.is_some(),
            ActionDataField::Charge => data.charge.is_some(),
            ActionDataField::PassiveModifier => data.passive_modifier.is_some(),
            ActionDataField::PassiveWhileReady => data.passive_while_ready,
            ActionDataField::Delivery => data.delivery.is_some(),
            ActionDataField::Rate => data.rate.is_some(),
            ActionDataField::Damage => data.damage.is_some(),
            ActionDataField::DamageKind => data.damage_kind.is_some(),
            ActionDataField::UnitType => data.unit_type.is_some(),
            ActionDataField::Requires => data.requires.is_some(),
            ActionDataField::Construct => data.construct.is_some(),
            ActionDataField::StartLife => data.start_life.is_some(),
            ActionDataField::CancelRefund => data.cancel_refund.is_some(),
            ActionDataField::Placement => data.placement.is_some(),
            ActionDataField::Resource => data.resource.is_some(),
            ActionDataField::Take => data.take.is_some(),
            ActionDataField::Bounce => data.bounce.is_some(),
            ActionDataField::Params => !data.params.is_empty(),
            ActionDataField::OnResolve => !data.on_resolve.is_empty(),
            ActionDataField::OnHit => !data.on_hit.is_empty(),
            ActionDataField::OnEnd => !data.on_end.is_empty(),
        }
    }

    /// The first field `data` of kind `kind` gives that its kind refuses, or does not give that
    /// its kind needs.
    pub fn misused(data: &ActionData, kind: ActionKind) -> Option<ActionDataField> {
        ActionDataField::ALL
            .into_iter()
            .find(|field| match field.use_by(kind) {
                FieldUse::Takes => false,
                FieldUse::Needs => !field.given(data),
                FieldUse::Refuses => field.given(data),
            })
    }
}
