use campfire_script::{ApiBuilder, ApiOwner, MemberSpec, ScriptValue, View};
use campfire_units::HitHandle;

use crate::damage::{Damage, DamageCause};

/// A damage as a script holds it, `Damage` in scripts: read only.
#[derive(Debug, Clone)]
pub struct DamageHandle {
    damage: Damage,
    view: View,
}

impl DamageHandle {
    pub const fn new(damage: Damage, view: View) -> Self {
        Self { damage, view }
    }

    /// The `Damage` handle's fields.
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let field = |name, description| MemberSpec::field(ApiOwner::Damage, name, description);
        api.ty::<Self>("Damage")
            .bind(
                field("source", "the unit that dealt it, `()` when gone or none"),
                |d: &mut Self| d.view.unit_value(d.damage.source),
            )
            .bind(
                field("target", "the unit it is dealt to"),
                |d: &mut Self| d.view.unit_value(Some(d.damage.target)),
            )
            .bind(
                field("amount", "raw in `calc_damage`, final in a hook"),
                |d: &mut Self| d.damage.amount,
            )
            .bind(
                field("kind", "one of the mode's `[combat] damage_kinds`"),
                |d: &mut Self| d.view.names().name(d.damage.kind),
            )
            .bind(
                field("attack", "whether an attack dealt it"),
                |d: &mut Self| d.damage.cause.attack(),
            )
            .bind(
                field("extra", "whether `ctx.attack_hit` dealt it"),
                |d: &mut Self| matches!(d.damage.cause, DamageCause::ExtraAttack { .. }),
            )
            .bind(
                field(
                    "roll",
                    "its attack's random number, at least 0 and less than 1, `()` for other damage",
                ),
                |d: &mut Self| {
                    d.damage
                        .cause
                        .roll()
                        .map_or(ScriptValue::UNIT, ScriptValue::of)
                },
            )
            .bind(
                field(
                    "hit",
                    "how its projectile or area reached the target, `()` for damage none delivered",
                ),
                |d: &mut Self| {
                    d.damage.hit.map_or(ScriptValue::UNIT, |hit| {
                        ScriptValue::of(HitHandle::new(hit, d.view.clone()))
                    })
                },
            )
            .bind(
                field(
                    "ability",
                    "the action that dealt it: an ability, or an attack's weapon; `()` for none",
                ),
                |d: &mut Self| d.view.ability_value(d.damage.action),
            );
    }
}
