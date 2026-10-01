use campfire_script::rhai::{Dynamic, Engine, ImmutableString};

use crate::combat::damage::{Damage, DamageCause};
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

    /// The `Damage` handle's fields: `source`, `()` when gone or none, `target`, `amount`,
    /// `kind`, `attack`, `extra`, `crit`, and `ability`, `""` when none.
    pub(crate) fn register(engine: &mut Engine) {
        engine
            .register_type_with_name::<DamageHandle>("Damage")
            .register_get("source", |d: &mut DamageHandle| {
                d.damage
                    .source
                    .and_then(|source| d.view.unit(source))
                    .map_or(Dynamic::UNIT, Dynamic::from)
            })
            .register_get("target", |d: &mut DamageHandle| {
                d.view
                    .unit(d.damage.target)
                    .map_or(Dynamic::UNIT, Dynamic::from)
            })
            .register_get("amount", |d: &mut DamageHandle| d.damage.amount)
            .register_get("kind", |d: &mut DamageHandle| {
                d.view.damage_kind_name(d.damage.kind)
            })
            .register_get("attack", |d: &mut DamageHandle| d.damage.cause.attack())
            .register_get("extra", |d: &mut DamageHandle| {
                d.damage.cause == DamageCause::ExtraAttack
            })
            .register_get("crit", |d: &mut DamageHandle| d.damage.cause.crit())
            .register_get("ability", |d: &mut DamageHandle| {
                d.damage
                    .ability
                    .map_or_else(ImmutableString::new, |id| d.view.ability_name(id))
            });
    }
}
