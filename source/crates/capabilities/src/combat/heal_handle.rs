use campfire_script::rhai::{Dynamic, ImmutableString};
use campfire_sim::Capability;

use crate::combat::heal::{Heal, HealCause};
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::script_api::{ApiOwner, MemberSpec};
use crate::units::script_view::View;

/// A heal as a script holds it, `Heal` in scripts: read only.
#[derive(Debug, Clone)]
pub(crate) struct HealHandle {
    heal: Heal,
    view: View,
}

impl HealHandle {
    pub(crate) const fn new(heal: Heal, view: View) -> HealHandle {
        HealHandle { heal, view }
    }

    /// The `Heal` handle's fields.
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let field = |name, description| {
            MemberSpec::field(ApiOwner::Heal, name, description).capability(Capability::Combat)
        };
        api.ty::<HealHandle>("Heal")
            .bind(
                field("source", "the unit that gave it, `()` when gone or none"),
                |h: &mut HealHandle| {
                    h.heal
                        .source
                        .and_then(|source| h.view.unit(source))
                        .map_or(Dynamic::UNIT, Dynamic::from)
                },
            )
            .bind(
                field("target", "the unit it heals"),
                |h: &mut HealHandle| {
                    h.view
                        .unit(h.heal.target)
                        .map_or(Dynamic::UNIT, Dynamic::from)
                },
            )
            .bind(
                field("amount", "before `calc_heal` and the heal scale"),
                |h: &mut HealHandle| h.heal.amount,
            )
            .bind(
                field("leech", "whether its source's leech gave it"),
                |h: &mut HealHandle| h.heal.cause == HealCause::Leech,
            )
            .bind(
                field("ability", "the ability that gave it, `\"\"` when none"),
                |h: &mut HealHandle| {
                    h.heal
                        .ability
                        .map_or_else(ImmutableString::new, |id| h.view.ability_name(id))
                },
            );
    }
}
