use campfire_script::{
    ApiBuilder, ApiOwner, MaybeName, MemberSpec, ScriptHandle, View, plain_script_arg,
};
use campfire_values::ActionId;

use crate::heal::{Heal, HealCause};

/// A heal as a script holds it, `Heal` in scripts: read only.
#[derive(Debug, Clone)]
pub struct HealHandle {
    heal: Heal,
    view: View,
}

impl HealHandle {
    pub const fn new(heal: Heal, view: View) -> Self {
        Self { heal, view }
    }

    /// The `Heal` handle's fields.
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let field =
            |name, description| MemberSpec::field(ApiOwner::of::<Self>(), name, description);
        api.ty::<Self>()
            .bind(
                field("source", "the unit that gave it, `()` when gone or none"),
                |h: &mut Self| h.view.unit_value(h.heal.source),
            )
            .bind(field("target", "the unit it heals"), |h: &mut Self| {
                h.view.unit_value(Some(h.heal.target))
            })
            .bind(
                field("amount", "before `calc_heal` and the heal scale"),
                |h: &mut Self| h.heal.amount,
            )
            .bind(
                field("is_leech", "whether its source's leech gave it"),
                |h: &mut Self| h.heal.cause == HealCause::Leech,
            )
            .bind(
                field("action", "the action that gave it, `()` for none"),
                |h: &mut Self| MaybeName::from(h.heal.action.map(|id| h.view.names().name(id))),
            )
            .method(
                MemberSpec::method(
                    ApiOwner::of::<Self>(),
                    "is_action",
                    &[&["action"]],
                    "whether `action`, an action of the script's package, gave it",
                ),
                |h: Self, action: ActionId| Ok(h.heal.action == Some(action)),
            );
    }
}

plain_script_arg!(HealHandle);

impl ScriptHandle for HealHandle {
    const NAME: &'static str = "Heal";
    const TITLE: &'static str = "Heal `h`";
}
