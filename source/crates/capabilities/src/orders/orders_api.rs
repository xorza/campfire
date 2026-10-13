use campfire_sim::Position;

use crate::actions::actions_column::ActionsColumn;
use crate::orders::ai_data::AiData;
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
            MemberSpec::call(name, signature, description).roles(RoleSet::AI)
        };
        api.call(
            order(
                "order_attack",
                &[&["unit", "target"]],
                "`unit` attacks `target`, a living enemy that one of its weapons selects",
            ),
            |ctx: &mut Ctx, unit: Unit, target: Unit| OrdersApi::attack(ctx, &unit, &target),
        )
        .call(
            order(
                "order_follow_path",
                &[&["unit"]],
                "`unit` drops its target and walks its path again",
            ),
            |ctx: &mut Ctx, unit: Unit| OrdersApi::order(ctx, &unit, UnitOrder::FollowPath),
        )
        .call(
            order(
                "order_move",
                &[&["unit", "pos"]],
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
        .call(
            order(
                "order_reset",
                &[&["unit"]],
                "`unit` drops its target and walks home, taking no order until there, where its pools fill",
            ),
            |ctx: &mut Ctx, unit: Unit| {
                let reset = OrdersApi::order(ctx, &unit, UnitOrder::Reset);
                // The acting unit, which only a unit the mode spawned is, holds its spawn place
                // as a part the modules declare.
                debug_assert!(
                    reset.is_err() || unit.read(|row| row.spawn.is_some()),
                    "the acting unit holds its spawn place"
                );
                reset
            },
        )
        .hook(Hook::OnThink, Status::Runs(ApiVersion::FIRST))
        .data::<AiData>(&["ai", "think_ms"], &[])
        .shared_data(DataTable::UnitType, &["orders"], &[]);
    }

    /// Queues an attack of `unit` on `target`, a living enemy that one of its learned weapons
    /// selects, as the attack order of a player needs.
    fn attack(ctx: &Ctx, unit: &Unit, target: &Unit) -> Checked<()> {
        let view = ctx.view();
        unit.read(|ordered| {
            target.read(|aimed| {
                let relation = view.relation(ordered.team, aimed.team);
                if !aimed.alive || !relation.may_attack() {
                    return Err(ApiError::NotAnEnemy.fail());
                }
                if !ActionsColumn::armed_against(view, unit.row_index(), ordered, aimed) {
                    return Err(ApiError::NoAttack.fail());
                }
                Ok(())
            })
        })?;
        OrdersApi::order(ctx, unit, UnitOrder::Attack { target: target.id })
    }

    /// Queues `order` for `unit`, which must be the unit that thinks.
    fn order(ctx: &Ctx, unit: &Unit, order: UnitOrder) -> Checked<()> {
        if ctx.acting() != Some(unit.id) {
            return Err(ApiError::OtherUnit.fail().into());
        }
        ctx.queue(order)
    }
}
