use campfire_sim::Capability;

use crate::orders::ai_order::AiOrder;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::effect::Effect;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::hook::Hook;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::{DataTable, MemberSpec, Status};
use crate::units::unit::Unit;

/// The script API of `orders`: the orders an AI gives the unit that thinks.
#[derive(Debug)]
pub(crate) struct OrdersApi;

impl OrdersApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let order = |name, signature, description| {
            MemberSpec::call(name, signature, description)
                .roles(RoleSet::AI)
                .capability(Capability::Orders)
        };
        api.bind(
            order(
                "order_attack",
                "(unit, target)",
                "`unit`, which has an attack, attacks `target`, a living enemy",
            ),
            |ctx: &mut Ctx, unit: Unit, target: Unit| OrdersApi::attack(ctx, &unit, &target),
        )
        .bind(
            order(
                "order_follow_path",
                "(unit)",
                "`unit` drops its target and walks its path again",
            ),
            |ctx: &mut Ctx, unit: Unit| OrdersApi::order(ctx, &unit, AiOrder::FollowPath),
        )
        .plan(order("order_move", "(unit, pos)", "`unit` walks to `pos`"))
        .plan(order(
            "order_reset",
            "(unit)",
            "`unit` walks home, heals, and drops its target",
        ))
        .hook(Hook::Think, "(ctx, unit)", Status::Runs)
        .data(DataTable::Ai, &["ai", "think_ms"], &[]);
    }

    /// Queues an attack of `unit`, which has an attack, on `target`, a living enemy.
    fn attack(ctx: &Ctx, unit: &Unit, target: &Unit) -> Checked<()> {
        let (ordered, target) = (unit.row(), target.row());
        if ordered.attack_range.is_none() {
            return Err(ApiError::NoAttack.fail().into());
        }
        if !target.alive || !ordered.team.is_enemy_of(target.team) {
            return Err(ApiError::NotAnEnemy.fail().into());
        }
        OrdersApi::order(ctx, unit, AiOrder::Attack { target: target.id })
    }

    /// Queues `order` for `unit`, which must be the unit that thinks.
    fn order(ctx: &Ctx, unit: &Unit, order: AiOrder) -> Checked<()> {
        ctx.require(RoleSet::AI)?;
        if ctx.acting() != Some(unit.id) {
            return Err(ApiError::OtherUnit.fail().into());
        }
        ctx.queue(Effect::Order(order))
    }
}
