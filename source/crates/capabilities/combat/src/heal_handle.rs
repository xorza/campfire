use campfire_script::{ApiBuilder, ApiOwner, MemberSpec, View};

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
        let field = |name, description| MemberSpec::field(ApiOwner::Heal, name, description);
        api.ty::<Self>("Heal")
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
                field("leech", "whether its source's leech gave it"),
                |h: &mut Self| h.heal.cause == HealCause::Leech,
            )
            .bind(
                field("ability", "the ability that gave it, `()` for none"),
                |h: &mut Self| h.view.ability_value(h.heal.action),
            );
    }
}
