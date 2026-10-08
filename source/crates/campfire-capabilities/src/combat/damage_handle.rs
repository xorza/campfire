use campfire_script::rhai::Dynamic;
use campfire_sim::Capability;

use crate::combat::damage::{Damage, DamageCause};
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::script_api::api_owner::ApiOwner;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::units::hit_handle::HitHandle;
use crate::units::view::View;

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
                |d: &mut DamageHandle| d.view.unit_value(d.damage.source),
            )
            .bind(
                field("target", "the unit it is dealt to"),
                |d: &mut DamageHandle| d.view.unit_value(Some(d.damage.target)),
            )
            .bind(
                field("amount", "raw in `calc_damage`, final in a hook"),
                |d: &mut DamageHandle| d.damage.amount,
            )
            .bind(
                field("kind", "one of the mode's `[combat] damage_kinds`"),
                |d: &mut DamageHandle| {
                    let name = d.view.damage_kind_name(d.damage.kind);
                    name.expect("a damage's kind is one of the mode's")
                },
            )
            .bind(
                field("attack", "whether an attack dealt it"),
                |d: &mut DamageHandle| d.damage.cause.attack(),
            )
            .bind(
                field("extra", "whether `ctx.attack_hit` dealt it"),
                |d: &mut DamageHandle| matches!(d.damage.cause, DamageCause::ExtraAttack { .. }),
            )
            .bind(
                field(
                    "roll",
                    "its attack's random number, at least 0 and less than 1, `()` for other damage",
                ),
                |d: &mut DamageHandle| d.damage.cause.roll().map_or(Dynamic::UNIT, Dynamic::from),
            )
            .bind(
                field(
                    "hit",
                    "how its projectile or area reached the target, `()` for damage none delivered",
                ),
                |d: &mut DamageHandle| {
                    d.damage.hit.map_or(Dynamic::UNIT, |hit| {
                        Dynamic::from(HitHandle::new(hit, d.view.clone()))
                    })
                },
            )
            .bind(
                field(
                    "ability",
                    "the action that dealt it: an ability, or an attack's weapon; `()` for none",
                ),
                |d: &mut DamageHandle| d.view.ability_value(d.damage.ability),
            );
    }
}
