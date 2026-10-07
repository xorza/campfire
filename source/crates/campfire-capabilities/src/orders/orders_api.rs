use campfire_sim::{Capability, Position};

use crate::actions::actions_column::ActionsColumn;
use crate::orders::unit_order::UnitOrder;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::hook::Hook;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::data_table::DataTable;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::scripts::script_api::status::Status;
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
                "`unit` attacks `target`, a living enemy that one of its weapons selects",
            ),
            |ctx: &mut Ctx, unit: Unit, target: Unit| OrdersApi::attack(ctx, &unit, &target),
        )
        .bind(
            order(
                "order_follow_path",
                "(unit)",
                "`unit` drops its target and walks its path again",
            ),
            |ctx: &mut Ctx, unit: Unit| OrdersApi::order(ctx, &unit, UnitOrder::FollowPath),
        )
        .bind(
            order(
                "order_move",
                "(unit, pos)",
                "`unit` drops its target and walks to `pos`, within the map, off its path",
            ),
            |ctx: &mut Ctx, unit: Unit, to: Position| {
                let to = to.get();
                OrdersApi::order(ctx, &unit, UnitOrder::Move {
                        x: to.x,
                        z: to.z,
                        party: None,
                    })
            },
        )
        .bind(
            order(
                "order_reset",
                "(unit)",
                "`unit` drops its target and walks home, taking no order until there, where its pools fill",
            ),
            |ctx: &mut Ctx, unit: Unit| {
                if unit.row().spawn.is_none() {
                    return Err(ApiError::NoSpawnPlace.fail().into());
                }
                OrdersApi::order(ctx, &unit, UnitOrder::Reset)
            },
        )
        .hook(Hook::OnThink, Status::Runs(ApiVersion::FIRST))
        .data(DataTable::Ai, &["ai", "think_ms"], &[]);
    }

    /// Queues an attack of `unit` on `target`, a living enemy that one of its learned weapons
    /// selects, as the attack order of a player needs.
    fn attack(ctx: &Ctx, unit: &Unit, target: &Unit) -> Checked<()> {
        let (ordered, target) = (unit.row(), target.row());
        let attitude = ctx.view().attitude(ordered.team, target.team);
        if !target.alive || !attitude.may_attack() {
            return Err(ApiError::NotAnEnemy.fail().into());
        }
        if !ActionsColumn::armed_against(ctx.view(), unit.row_index(), &ordered, &target) {
            return Err(ApiError::NoAttack.fail().into());
        }
        OrdersApi::order(ctx, unit, UnitOrder::Attack { target: target.id })
    }

    /// Queues `order` for `unit`, which must be the unit that thinks.
    fn order(ctx: &Ctx, unit: &Unit, order: UnitOrder) -> Checked<()> {
        ctx.require(RoleSet::AI)?;
        if ctx.acting() != Some(unit.id) {
            return Err(ApiError::OtherUnit.fail().into());
        }
        ctx.queue(order)
    }
}
