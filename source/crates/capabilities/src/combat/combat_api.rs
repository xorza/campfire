use campfire_math::Num;
use campfire_script::rhai::INT;
use campfire_sim::Capability;

use crate::combat::combat_effect::CombatEffect;
use crate::combat::damage_handle::DamageHandle;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::effect::Effect;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::{DataTable, MemberSpec, Status};
use crate::units::block::Block;
use crate::units::tag_effect::TagEffect;
use crate::units::unit::Unit;

/// The script API of `combat`: `ctx.damage`, `ctx.heal`, `ctx.restore`, `ctx.attack_hit`, and the
/// `Damage` handle.
#[derive(Debug)]
pub(crate) struct CombatApi;

impl CombatApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        DamageHandle::register(api);
        let call = |name, signature, description| {
            MemberSpec::call(name, signature, description).capability(Capability::Combat)
        };
        let damage = call(
            "damage",
            "(target, amount, kind)",
            "deals `amount` of `kind`, one of the mode's `[combat] damage_kinds`, to `target`",
        );
        let heal = call(
            "heal",
            "(unit, amount)",
            "heals `unit`'s life pool, times one plus its `heal_scale` stat",
        );
        let restore = call(
            "restore",
            "(unit, pool, amount)",
            "gives `unit` back `amount` of its `pool`, unscaled",
        );
        let attack_hit = call(
            "attack_hit",
            "(target)",
            "an extra attack of the acting unit on `target`: no crit, and no `on_attack`",
        )
        .roles(RoleSet::ACTING);
        api.bind(
            damage,
            |ctx: &mut Ctx, target: Unit, amount: Num, kind: &str| {
                CombatApi::damage(ctx, &target, amount, kind)
            },
        )
        .bind(
            damage,
            |ctx: &mut Ctx, target: Unit, amount: INT, kind: &str| {
                CombatApi::damage(ctx, &target, ApiError::num(amount)?, kind)
            },
        )
        .bind(heal, |ctx: &mut Ctx, unit: Unit, amount: Num| {
            CombatApi::mend(ctx, amount, |amount| CombatEffect::Heal {
                unit: unit.id,
                amount,
            })
        })
        .bind(heal, |ctx: &mut Ctx, unit: Unit, amount: INT| {
            let amount = ApiError::num(amount)?;
            CombatApi::mend(ctx, amount, |amount| CombatEffect::Heal {
                unit: unit.id,
                amount,
            })
        })
        .bind(
            restore,
            |ctx: &mut Ctx, unit: Unit, pool: &str, amount: Num| {
                CombatApi::restore(ctx, &unit, pool, amount)
            },
        )
        .bind(
            restore,
            |ctx: &mut Ctx, unit: Unit, pool: &str, amount: INT| {
                CombatApi::restore(ctx, &unit, pool, ApiError::num(amount)?)
            },
        )
        .bind(attack_hit, |ctx: &mut Ctx, target: Unit| {
            CombatApi::attack_hit(ctx, &target)
        })
        .tag_effect(TagEffect::Blocks(Block::Attack), Status::Runs)
        .tag_effect(TagEffect::Blocks(Block::Target), Status::Runs)
        .tag_effect(TagEffect::Blocks(Block::Damage), Status::Runs)
        .data(
            DataTable::ModeCombat,
            &[
                "damage_kinds",
                "assist_window_ms",
                "life",
                "leech",
                "heal_scale",
            ],
            &[],
        )
        .data(DataTable::Leech, &["attack", "other"], &[])
        .data(DataTable::Combat, &["on_death"], &[])
        .data(
            DataTable::Action,
            &["rate", "damage", "damage_kind", "projectile"],
            &[],
        )
        .data(
            DataTable::Projectile,
            &["speed"],
            &[
                "width",
                "range",
                "stop_on_hit",
                "once_per_cast",
                "hits",
                "sight_radius",
                "collide",
            ],
        );
    }

    /// Queues `amount` of `kind` damage to `target`, a kind the mode declares.
    fn damage(ctx: &Ctx, target: &Unit, amount: Num, kind: &str) -> Checked<()> {
        let kind = ctx.view().damage_kind(kind)?;
        if amount < Num::ZERO {
            return Err(ApiError::NegativeDamage.fail().into());
        }
        let target = target.id;
        ctx.queue(Effect::Combat(CombatEffect::Damage {
            target,
            amount,
            kind,
        }))
    }

    /// Queues a restore of `amount` of `unit`'s pool `name`, a pool the mode declares.
    fn restore(ctx: &Ctx, unit: &Unit, name: &str, amount: Num) -> Checked<()> {
        let pool = ctx.view().pool(name)?;
        CombatApi::mend(ctx, amount, |amount| CombatEffect::Restore {
            unit: unit.id,
            pool,
            amount,
        })
    }

    /// Queues the heal or restore `effect` makes of `amount`, which is not negative.
    fn mend(ctx: &Ctx, amount: Num, effect: impl FnOnce(Num) -> CombatEffect) -> Checked<()> {
        if amount < Num::ZERO {
            return Err(ApiError::NegativeHeal.fail().into());
        }
        ctx.queue(Effect::Combat(effect(amount)))
    }

    /// Queues an extra attack of the acting unit, which has an attack, on `target`.
    fn attack_hit(ctx: &Ctx, target: &Unit) -> Checked<()> {
        let attacks = ctx
            .acting()
            .and_then(|id| ctx.view().row(id))
            .is_some_and(|row| row.attack_range.is_some());
        if !attacks {
            return Err(ApiError::NoAttack.fail().into());
        }
        ctx.queue(Effect::Combat(CombatEffect::AttackHit {
            target: target.id,
        }))
    }
}
