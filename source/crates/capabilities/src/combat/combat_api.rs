use campfire_math::Num;
use campfire_sim::Capability;

use crate::actions::action_data_field::ActionDataField;
use crate::actions::actions_column::ActionsColumn;
use crate::combat::combat_column::CombatColumn;
use crate::combat::combat_data::CombatData;
use crate::combat::combat_effect::CombatEffect;
use crate::combat::damage_handle::DamageHandle;
use crate::combat::heal_handle::HealHandle;
use crate::combat::mode_combat_data::LeechData;
use crate::combat::mode_combat_data::ModeCombatData;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::hook::Hook;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::api_owner::ApiOwner;
use crate::scripts::script_api::data_table::DataTable;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::scripts::script_api::status::Status;
use crate::stats::pool_id::PoolId;
use crate::units::block::Block;
use crate::units::tag_property::TagProperty;
use crate::units::unit::Unit;
use crate::values::damage_kind::DamageKind;

/// The script API of `combat`: `ctx.damage`, `ctx.heal`, `ctx.restore`, `ctx.attack_hit`, and the
/// `Damage` handle.
#[derive(Debug)]
pub(crate) struct CombatApi;

impl CombatApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        DamageHandle::register(api);
        HealHandle::register(api);
        CombatApi::register_unit(api);
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
        let attack_hit = call(
            "attack_hit",
            &[&["target"]],
            "an extra attack of the acting unit on `target`: no crit, and no `on_attack`",
        )
        .roles(RoleSet::ACTING);
        api.call(
            damage,
            |ctx: &mut Ctx, target: Unit, amount: Num, kind: DamageKind| {
                CombatApi::damage(ctx, &target, amount, kind)
            },
        )
        .call(heal, |ctx: &mut Ctx, unit: Unit, amount: Num| {
            CombatApi::mend(ctx, amount, |amount| CombatEffect::Heal {
                unit: unit.id,
                amount,
            })
        })
        .call(
            restore,
            |ctx: &mut Ctx, unit: Unit, pool: PoolId, amount: Num| {
                CombatApi::mend(ctx, amount, |amount| CombatEffect::Restore {
                    id: unit.id,
                    pool,
                    amount,
                })
            },
        )
        .call(attack_hit, |ctx: &mut Ctx, target: Unit| {
            CombatApi::attack_hit(ctx, &target)
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
            ],
            &[],
        )
        .data::<LeechData>(&["attack", "other"], &[])
        .data::<CombatData>(&["on_death"], &[])
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
        api.method(recent_attackers, |unit: Unit, ms: i64| {
            CombatColumn::recent_attackers(&unit, ms)
        });
    }

    /// Queues `amount` of `kind` damage to `target`.
    fn damage(ctx: &Ctx, target: &Unit, amount: Num, kind: DamageKind) -> Checked<()> {
        if amount < Num::ZERO {
            return Err(ApiError::NegativeDamage.fail().into());
        }
        let target = target.id;
        ctx.queue(CombatEffect::Damage {
            target,
            amount,
            kind,
        })
    }

    /// Queues the heal or restore `effect` makes of `amount`, which is not negative.
    fn mend(ctx: &Ctx, amount: Num, effect: impl FnOnce(Num) -> CombatEffect) -> Checked<()> {
        if amount < Num::ZERO {
            return Err(ApiError::NegativeHeal.fail().into());
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
        ctx.queue(CombatEffect::AttackHit { target: target.id })
    }
}
