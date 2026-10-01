use campfire_script::rhai::{Dynamic, ImmutableString};
use campfire_sim::Capability;

use crate::combat::damage::{Damage, DamageCause};
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::script_api::{ApiOwner, MemberSpec};
use crate::units::script_view::View;

/// A damage as a script holds it, `Damage` in scripts: read only.
#[derive(Debug, Clone)]
pub(crate) struct DamageHandle {
    damage: Damage,
    view: View,
}

impl DamageHandle {
    pub(crate) const fn new(damage: Damage, view: View) -> DamageHandle {
        DamageHandle { damage, view }
    }

    /// The `Damage` handle's fields.
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let field = |name, description| {
            MemberSpec::field(ApiOwner::Damage, name, description).capability(Capability::Combat)
        };
        api.ty::<DamageHandle>("Damage")
            .bind(
                field("source", "the unit that dealt it, `()` when gone or none"),
                |d: &mut DamageHandle| {
                    d.damage
                        .source
                        .and_then(|source| d.view.unit(source))
                        .map_or(Dynamic::UNIT, Dynamic::from)
                },
            )
            .bind(
                field("target", "the unit it is dealt to"),
                |d: &mut DamageHandle| {
                    d.view
                        .unit(d.damage.target)
                        .map_or(Dynamic::UNIT, Dynamic::from)
                },
            )
            .bind(
                field("amount", "raw in `calc_damage`, final in a hook"),
                |d: &mut DamageHandle| d.damage.amount,
            )
            .bind(
                field("kind", "one of the mode's `damage_kinds`"),
                |d: &mut DamageHandle| d.view.damage_kind_name(d.damage.kind),
            )
            .bind(
                field("attack", "whether an attack dealt it"),
                |d: &mut DamageHandle| d.damage.cause.attack(),
            )
            .bind(
                field("extra", "whether `ctx.attack_hit` dealt it"),
                |d: &mut DamageHandle| d.damage.cause == DamageCause::ExtraAttack,
            )
            .bind(
                field("crit", "whether its attack crit"),
                |d: &mut DamageHandle| d.damage.cause.crit(),
            )
            .bind(
                field("ability", "the ability that dealt it, `\"\"` when none"),
                |d: &mut DamageHandle| {
                    d.damage
                        .ability
                        .map_or_else(ImmutableString::new, |id| d.view.ability_name(id))
                },
            );
    }
}
