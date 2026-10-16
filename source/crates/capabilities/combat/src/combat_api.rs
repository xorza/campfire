use campfire_actions::{ActionDataField, ActionsColumn};
use campfire_math::Num;
use campfire_script::{
    ApiBuilder, ApiError, ApiOwner, ApiVersion, CapabilityRefusal, Checked, Ctx, DataTable, Hook,
    MemberSpec, RoleSet, ScriptWindow, Status, Unit,
};
use campfire_sim::Capability;
use campfire_stats::PoolId;
use campfire_values::{Block, DamageKind, TagProperty};

use crate::combat_column::CombatColumn;
use crate::combat_data::CombatData;
use crate::combat_effect::CombatEffect;
use crate::damage_handle::DamageHandle;
use crate::error::CombatRefusal;
use crate::heal_handle::HealHandle;
use crate::mode_combat_data::{LeechData, ModeCombatData};

/// The script API of `combat`: `ctx.damage`, `ctx.heal`, `ctx.restore`, `ctx.attack_hit`, and the
/// `Damage` handle.
#[derive(Debug)]
pub struct CombatApi;

impl CombatApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        DamageHandle::register(api);
        HealHandle::register(api);
        Self::register_unit(api);
        let call = |name, signature, description| MemberSpec::call(name, signature, description);
        let damage = call(
            "damage",
            &[&["target", "amount", "kind"]],
            "deals `amount` of `kind`, one of the mode's `[combat] damage_kinds`, to `target`",
        );
        let heal = call(
            "heal",
            &[&["unit", "amount"]],
            "heals `unit`'s life pool, times one plus its `heal_scale` stat",
        );
        let restore = call(
            "restore",
            &[&["unit", "pool", "amount"]],
            "gives `unit` back `amount` of its `pool`, unscaled",
        );
        let kill = call(
            "kill",
            &[&["unit"]],
            "takes `unit`'s life whole: it dies as the next tick's deaths find it, with no killer; \
             nothing for a unit that does not live",
        )
        .roles(RoleSet::MODE);
        let attack_hit = call(
            "attack_hit",
            &[&["target"]],
            "an extra attack of the acting unit on `target`: no crit, and no `on_attack`",
        )
        .roles(RoleSet::ACTING);
        api.call(
            damage,
            |ctx: &mut Ctx, target: Unit, amount: Num, kind: DamageKind| {
                Self::damage(ctx, &target, amount, kind)
            },
        )
        .call(heal, |ctx: &mut Ctx, unit: Unit, amount: Num| {
            Self::mend(ctx, amount, |amount| CombatEffect::Heal {
                unit: unit.id,
                amount,
            })
        })
        .call(
            restore,
            |ctx: &mut Ctx, unit: Unit, pool: PoolId, amount: Num| {
                Self::mend(ctx, amount, |amount| CombatEffect::Restore {
                    unit: unit.id,
                    pool,
                    amount,
                })
            },
        )
        .call(attack_hit, |ctx: &mut Ctx, target: Unit| {
            Self::attack_hit(ctx, &target)
        })
        .call(kill, |ctx: &mut Ctx, unit: Unit| {
            ctx.queue(CombatEffect::Kill { unit: unit.id })
        })
        .tag_property(
            TagProperty::Blocks(Block::Attack),
            Status::Runs(ApiVersion::FIRST),
        )
        .tag_property(
            TagProperty::Blocks(Block::Target),
            Status::Runs(ApiVersion::FIRST),
        )
        .tag_property(
            TagProperty::Blocks(Block::Damage),
            Status::Runs(ApiVersion::FIRST),
        )
        .data::<ModeCombatData>(
            &[
                "damage_kinds",
                "assist_window_ms",
                "life",
                "leech",
                "heal_scale",
                "armors",
            ],
            &[],
        )
        .data::<LeechData>(&["attack", "other"], &[])
        .data::<CombatData>(&["on_death", "armor"], &[])
        .shared_data(DataTable::Mode, &["combat"], &[])
        .shared_data(DataTable::UnitType, &["combat"], &[])
        .action_fields(ActionDataField::of(Some(Capability::Combat)))
        .hook(Hook::OnAttack, Status::Runs(ApiVersion::FIRST))
        .hook(Hook::OnAttackHit, Status::Runs(ApiVersion::FIRST))
        .hook(Hook::OnDamageTaken, Status::Runs(ApiVersion::FIRST))
        .hook(Hook::OnKill, Status::Runs(ApiVersion::FIRST))
        .hook(Hook::OnTakedown, Status::Runs(ApiVersion::FIRST))
        .hook(Hook::CalcDamage, Status::Runs(ApiVersion::FIRST))
        .hook(Hook::CalcHeal, Status::Runs(ApiVersion::FIRST));
    }

    /// What combat adds to a `Unit` handle.
    fn register_unit(api: &mut ApiBuilder<'_>) {
        let recent_attackers = MemberSpec::method(
            ApiOwner::Unit,
            "recent_attackers",
            &[&["ms"]],
            "the living units that struck it within the last `ms`, rounded up to whole ticks",
        );
        api.method(recent_attackers, |unit: Unit, ms: ScriptWindow| {
            Ok(CombatColumn::recent_attackers(&unit, ms))
        });
    }

    /// Queues `amount` of `kind` damage to `target`.
    fn damage(ctx: &Ctx, target: &Unit, amount: Num, kind: DamageKind) -> Checked<()> {
        if amount < Num::ZERO {
            return Err(CombatRefusal::NegativeDamage.fail().into());
        }
        ctx.queue(CombatEffect::Damage {
            unit: target.id,
            amount,
            kind,
        })
    }

    /// Queues the heal or restore `effect` makes of `amount`, which is not negative.
    fn mend(ctx: &Ctx, amount: Num, effect: impl FnOnce(Num) -> CombatEffect) -> Checked<()> {
        if amount < Num::ZERO {
            return Err(CombatRefusal::NegativeHeal.fail().into());
        }
        ctx.queue(effect(amount))
    }

    /// Queues an extra attack of the acting unit, which has an attack, on `target`.
    fn attack_hit(ctx: &Ctx, target: &Unit) -> Checked<()> {
        let view = ctx.view();
        let attacks = ctx
            .acting()
            .and_then(|id| view.row_index(id))
            .is_some_and(|row| ActionsColumn::attack_range(view, row).is_some());
        if !attacks {
            return Err(ApiError::NoAttack.fail().into());
        }
        ctx.queue(CombatEffect::AttackHit { unit: target.id })
    }
}
