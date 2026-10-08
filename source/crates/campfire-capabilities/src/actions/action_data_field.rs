use campfire_sim::Capability;
use campfire_sim::Capability::{Abilities, Combat, Production, Projectiles};

use crate::actions::action_data::ActionData;
use crate::actions::action_data_field::FieldUse::{Needs, Refuses, Takes};
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

/// A field's row of the table: the field, its name as data writes it, the capability that runs
/// it, none for the core's, its use by each kind the release runs, and whether data gives it: a
/// value, a list or a table that is not empty, or a flag that is on.
#[derive(Debug, Clone, Copy)]
struct FieldRule {
    field: ActionDataField,
    name: &'static str,
    capability: Option<Capability>,
    uses: [FieldUse; KINDS.len()],
    given: fn(&ActionData) -> bool,
}

/// The table of action fields, a row each, in the order of the fields.
const TABLE: [FieldRule; 32] = [
    FieldRule {
        field: ActionDataField::Kind,
        name: "kind",
        capability: None,
        uses: [Takes, Takes, Takes, Takes, Takes],
        given: |_| true,
    },
    FieldRule {
        field: ActionDataField::Script,
        name: "script",
        capability: Some(Abilities),
        uses: [Takes, Refuses, Refuses, Refuses, Refuses],
        given: |data| data.script.is_some(),
    },
    FieldRule {
        field: ActionDataField::Targeting,
        name: "targeting",
        capability: None,
        uses: [Takes, Takes, Takes, Takes, Takes],
        given: |_| true,
    },
    FieldRule {
        field: ActionDataField::Range,
        name: "range",
        capability: None,
        uses: [Takes, Needs, Refuses, Needs, Needs],
        given: |data| data.range.is_some(),
    },
    FieldRule {
        field: ActionDataField::CooldownMs,
        name: "cooldown_ms",
        capability: Some(Abilities),
        uses: [Takes, Refuses, Takes, Takes, Refuses],
        given: |data| data.cooldown_ms.is_some(),
    },
    FieldRule {
        field: ActionDataField::Cost,
        name: "cost",
        capability: None,
        uses: [Takes, Takes, Takes, Takes, Refuses],
        given: |data| !data.cost.is_empty(),
    },
    FieldRule {
        field: ActionDataField::WindupMs,
        name: "windup_ms",
        capability: None,
        uses: [Takes, Takes, Takes, Needs, Needs],
        given: |data| data.windup_ms.is_some(),
    },
    FieldRule {
        field: ActionDataField::ClampToRange,
        name: "clamp_to_range",
        capability: Some(Abilities),
        uses: [Takes, Refuses, Refuses, Refuses, Refuses],
        given: |data| data.clamp_to_range,
    },
    FieldRule {
        field: ActionDataField::Toggle,
        name: "toggle",
        capability: Some(Abilities),
        uses: [Takes, Refuses, Refuses, Refuses, Refuses],
        given: |data| data.toggle.is_some(),
    },
    FieldRule {
        field: ActionDataField::Channel,
        name: "channel",
        capability: Some(Abilities),
        uses: [Takes, Refuses, Refuses, Refuses, Refuses],
        given: |data| data.channel.is_some(),
    },
    FieldRule {
        field: ActionDataField::Hold,
        name: "hold",
        capability: Some(Abilities),
        uses: [Takes, Refuses, Refuses, Refuses, Refuses],
        given: |data| data.hold.is_some(),
    },
    FieldRule {
        field: ActionDataField::Charges,
        name: "charges",
        capability: Some(Abilities),
        uses: [Takes, Refuses, Refuses, Refuses, Refuses],
        given: |data| data.charges.is_some(),
    },
    FieldRule {
        field: ActionDataField::Charge,
        name: "charge",
        capability: Some(Abilities),
        uses: [Takes, Refuses, Refuses, Refuses, Refuses],
        given: |data| data.charge.is_some(),
    },
    FieldRule {
        field: ActionDataField::PassiveModifier,
        name: "passive_modifier",
        capability: None,
        uses: [Takes, Takes, Takes, Takes, Takes],
        given: |data| data.passive_modifier.is_some(),
    },
    FieldRule {
        field: ActionDataField::PassiveWhileReady,
        name: "passive_while_ready",
        capability: None,
        uses: [Takes, Takes, Takes, Takes, Takes],
        given: |data| data.passive_while_ready,
    },
    FieldRule {
        field: ActionDataField::Delivery,
        name: "delivery",
        capability: Some(Projectiles),
        uses: [Takes, Takes, Refuses, Refuses, Refuses],
        given: |data| data.delivery.is_some(),
    },
    FieldRule {
        field: ActionDataField::Rate,
        name: "rate",
        capability: Some(Combat),
        uses: [Refuses, Needs, Refuses, Refuses, Refuses],
        given: |data| data.rate.is_some(),
    },
    FieldRule {
        field: ActionDataField::Damage,
        name: "damage",
        capability: Some(Combat),
        uses: [Refuses, Needs, Refuses, Refuses, Refuses],
        given: |data| data.damage.is_some(),
    },
    FieldRule {
        field: ActionDataField::DamageKind,
        name: "damage_kind",
        capability: Some(Combat),
        uses: [Refuses, Needs, Refuses, Refuses, Refuses],
        given: |data| data.damage_kind.is_some(),
    },
    FieldRule {
        field: ActionDataField::UnitType,
        name: "unit_type",
        capability: Some(Production),
        uses: [Refuses, Refuses, Needs, Needs, Refuses],
        given: |data| data.unit_type.is_some(),
    },
    FieldRule {
        field: ActionDataField::Requires,
        name: "requires",
        capability: Some(Production),
        uses: [Refuses, Refuses, Takes, Takes, Refuses],
        given: |data| data.requires.is_some(),
    },
    FieldRule {
        field: ActionDataField::Construct,
        name: "construct",
        capability: Some(Production),
        uses: [Refuses, Refuses, Refuses, Needs, Refuses],
        given: |data| data.construct.is_some(),
    },
    FieldRule {
        field: ActionDataField::StartLife,
        name: "start_life",
        capability: Some(Production),
        uses: [Refuses, Refuses, Refuses, Takes, Refuses],
        given: |data| data.start_life.is_some(),
    },
    FieldRule {
        field: ActionDataField::CancelRefund,
        name: "cancel_refund",
        capability: Some(Production),
        uses: [Refuses, Refuses, Refuses, Takes, Refuses],
        given: |data| data.cancel_refund.is_some(),
    },
    FieldRule {
        field: ActionDataField::Placement,
        name: "placement",
        capability: Some(Production),
        uses: [Refuses, Refuses, Refuses, Takes, Refuses],
        given: |data| data.placement.is_some(),
    },
    FieldRule {
        field: ActionDataField::Resource,
        name: "resource",
        capability: Some(Production),
        uses: [Refuses, Refuses, Refuses, Refuses, Needs],
        given: |data| data.resource.is_some(),
    },
    FieldRule {
        field: ActionDataField::Take,
        name: "take",
        capability: Some(Production),
        uses: [Refuses, Refuses, Refuses, Refuses, Needs],
        given: |data| data.take.is_some(),
    },
    FieldRule {
        field: ActionDataField::Bounce,
        name: "bounce",
        capability: Some(Production),
        uses: [Refuses, Refuses, Refuses, Refuses, Takes],
        given: |data| data.bounce.is_some(),
    },
    FieldRule {
        field: ActionDataField::Params,
        name: "params",
        capability: None,
        uses: [Takes, Takes, Refuses, Refuses, Refuses],
        given: |data| !data.params.is_empty(),
    },
    FieldRule {
        field: ActionDataField::OnResolve,
        name: "on_resolve",
        capability: None,
        uses: [Takes, Refuses, Refuses, Refuses, Refuses],
        given: |data| !data.on_resolve.is_empty(),
    },
    FieldRule {
        field: ActionDataField::OnHit,
        name: "on_hit",
        capability: None,
        uses: [Takes, Takes, Refuses, Refuses, Refuses],
        given: |data| !data.on_hit.is_empty(),
    },
    FieldRule {
        field: ActionDataField::OnEnd,
        name: "on_end",
        capability: None,
        uses: [Takes, Refuses, Refuses, Refuses, Refuses],
        given: |data| !data.on_end.is_empty(),
    },
];

impl ActionDataField {
    /// The fields `capability` runs, none for the action pipeline's, each by its name with its
    /// status in the script API.
    pub(crate) fn of(
        capability: Option<Capability>,
    ) -> impl Iterator<Item = (&'static str, Status)> {
        ActionDataField::ALL
            .into_iter()
            .filter(move |field| field.capability() == capability)
            .map(|field| (field.name(), Status::Runs(ApiVersion::FIRST)))
    }

    /// Every field, in the table's order, which is theirs.
    pub const ALL: [ActionDataField; TABLE.len()] = {
        let mut all = [ActionDataField::Kind; TABLE.len()];
        let mut at = 0;
        while at < all.len() {
            assert!(
                TABLE[at].field as usize == at,
                "the table's rows follow the fields"
            );
            all[at] = TABLE[at].field;
            at += 1;
        }
        all
    };

    const fn rule(self) -> &'static FieldRule {
        &TABLE[self as usize]
    }

    /// The field's name, as data writes it.
    pub const fn name(self) -> &'static str {
        self.rule().name
    }

    /// The capability that runs it; `None` for a field of the core.
    pub const fn capability(self) -> Option<Capability> {
        self.rule().capability
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
    pub(crate) fn given(self, data: &ActionData) -> bool {
        (self.rule().given)(data)
    }

    /// The first field `data` of kind `kind` gives that its kind refuses, or does not give that
    /// its kind needs.
    pub(crate) fn misused(data: &ActionData, kind: ActionKind) -> Option<ActionDataField> {
        ActionDataField::ALL
            .into_iter()
            .find(|field| match field.use_by(kind) {
                Takes => false,
                Needs => !field.given(data),
                Refuses => field.given(data),
            })
    }
}
