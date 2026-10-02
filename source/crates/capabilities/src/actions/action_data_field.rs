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
    Params,
    ProjectileState,
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
const KINDS: [ActionKind; 3] = [ActionKind::Cast, ActionKind::Attack, ActionKind::Train];

/// A field's row of the table: its name as data writes it, the capability that runs it, none for
/// the core's, whether the release runs it, and its use by each kind the release runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FieldRule {
    name: &'static str,
    capability: Option<Capability>,
    runs: bool,
    uses: [FieldUse; KINDS.len()],
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

    pub const ALL: [ActionDataField; 25] = [
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
        ActionDataField::Params,
        ActionDataField::ProjectileState,
        ActionDataField::OnResolve,
        ActionDataField::OnHit,
        ActionDataField::OnEnd,
    ];

    const fn rule(self) -> FieldRule {
        use Capability::{Abilities, Combat, Production, Projectiles};
        use FieldUse::{Needs, Refuses, Takes};
        let (name, capability, runs, uses) = match self {
            ActionDataField::Kind => ("kind", None, true, [Takes, Takes, Takes]),
            ActionDataField::Script => ("script", Some(Abilities), true, [Takes, Refuses, Refuses]),
            ActionDataField::Targeting => ("targeting", None, true, [Takes, Takes, Takes]),
            ActionDataField::Range => ("range", None, true, [Takes, Needs, Refuses]),
            ActionDataField::CooldownMs => (
                "cooldown_ms",
                Some(Abilities),
                true,
                [Takes, Refuses, Takes],
            ),
            ActionDataField::Cost => ("cost", None, true, [Takes, Takes, Takes]),
            ActionDataField::WindupMs => ("windup_ms", None, true, [Takes, Takes, Takes]),
            ActionDataField::ClampToRange => (
                "clamp_to_range",
                Some(Abilities),
                false,
                [Takes, Refuses, Refuses],
            ),
            ActionDataField::Toggle => {
                ("toggle", Some(Abilities), false, [Takes, Refuses, Refuses])
            }
            ActionDataField::Channel => {
                ("channel", Some(Abilities), false, [Takes, Refuses, Refuses])
            }
            ActionDataField::Hold => ("hold", Some(Abilities), false, [Takes, Refuses, Refuses]),
            ActionDataField::Charges => {
                ("charges", Some(Abilities), false, [Takes, Refuses, Refuses])
            }
            ActionDataField::Charge => {
                ("charge", Some(Abilities), false, [Takes, Refuses, Refuses])
            }
            ActionDataField::PassiveModifier => {
                ("passive_modifier", None, true, [Takes, Takes, Takes])
            }
            ActionDataField::PassiveWhileReady => {
                ("passive_while_ready", None, true, [Takes, Takes, Takes])
            }
            ActionDataField::Delivery => {
                ("delivery", Some(Projectiles), true, [Takes, Takes, Refuses])
            }
            ActionDataField::Rate => ("rate", Some(Combat), true, [Refuses, Needs, Refuses]),
            ActionDataField::Damage => ("damage", Some(Combat), true, [Refuses, Needs, Refuses]),
            ActionDataField::DamageKind => {
                ("damage_kind", Some(Combat), true, [Refuses, Needs, Refuses])
            }
            ActionDataField::UnitType => (
                "unit_type",
                Some(Production),
                true,
                [Refuses, Refuses, Needs],
            ),
            ActionDataField::Params => ("params", Some(Abilities), true, [Takes, Refuses, Refuses]),
            ActionDataField::ProjectileState => (
                "projectile_state",
                Some(Abilities),
                false,
                [Takes, Refuses, Refuses],
            ),
            ActionDataField::OnResolve => (
                "on_resolve",
                Some(Abilities),
                true,
                [Takes, Refuses, Refuses],
            ),
            ActionDataField::OnHit => ("on_hit", Some(Abilities), true, [Takes, Refuses, Refuses]),
            ActionDataField::OnEnd => ("on_end", Some(Abilities), true, [Takes, Refuses, Refuses]),
        };
        FieldRule {
            name,
            capability,
            runs,
            uses,
        }
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
            ActionDataField::Params => !data.params.is_empty(),
            ActionDataField::ProjectileState => !data.projectile_state.is_empty(),
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
