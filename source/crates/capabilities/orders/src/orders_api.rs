use campfire_actions::ActionsColumn;
use campfire_script::{
    ApiBuilder, CapabilityRefusal, Checked, Ctx, MemberSpec, RoleSet, ScriptPosition, Unit,
};

use crate::ai_data::AiData;
use crate::error::OrdersRefusal;
use crate::orders_effect::OrdersEffect;
use crate::unit_order::UnitOrder;

/// The script API of `orders`: the orders an AI gives the unit that thinks.
#[derive(Debug)]
pub struct OrdersApi;

impl OrdersApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let order = |name, signature, description| {
            MemberSpec::call(name, signature, description).roles(RoleSet::AI)
        };
        api.call(
            order(
                "order_attack",
                &[&["target"]],
                "the unit that thinks attacks `target`, a living enemy that one of its weapons selects",
            ),
            |ctx: &mut Ctx, target: Unit| Self::attack(ctx, &target),
        )
        .call(
            order(
                "order_follow_path",
                &[&[]],
                "the unit that thinks drops its target and walks its path again",
            ),
            |ctx: &mut Ctx| ctx.queue(OrdersEffect(UnitOrder::FollowPath)),
        )
        .call(
            order(
                "order_move",
                &[&["pos"]],
                "the unit that thinks drops its target and walks to `pos`, within the map, off its path",
            ),
            |ctx: &mut Ctx, to: ScriptPosition| {
                let to = to.get();
                ctx.queue(OrdersEffect(UnitOrder::Move {
                    x: to.x,
                    z: to.z,
                    party: None,
                }))
            },
        )
        .call(
            order(
                "order_reset",
                &[&[]],
                "the unit that thinks drops its target and walks home, taking no order until there, where its pools fill",
            ),
            |ctx: &mut Ctx| ctx.queue(OrdersEffect(UnitOrder::Reset)),
        )
        .plan(order(
            "order_action",
            &[&["slot", "target"]],
            "the unit that thinks casts the action in its slot `slot` at `target`, as a player's action order does",
        ))
        .data::<AiData>(&[]);
    }

    /// Queues an attack of the unit that thinks on `target`, a living enemy that one of its
    /// learned weapons selects, as the attack order of a player needs.
    fn attack(ctx: &Ctx, target: &Unit) -> Checked<()> {
        let view = ctx.view();
        let acting = ctx.acting().and_then(|id| view.unit(id));
        let unit = acting.expect("an AI's call acts as the unit that thinks");
        let (row, aimed) = (unit.row_index()?, target.row_index()?);
        let refusal = view.read_row(row, |ordered| {
            view.read_row(aimed, |aimed| {
                let relation = view.relation(ordered.team, aimed.team);
                if !aimed.alive || !relation.may_attack() {
                    return Some(OrdersRefusal::NotAnEnemy);
                }
                let armed = ActionsColumn::armed_against(view, row, ordered, aimed);
                (!armed).then_some(OrdersRefusal::NoAttack)
            })
        });
        if let Some(refusal) = refusal {
            return Err(refusal.fail().into());
        }
        ctx.queue(OrdersEffect(UnitOrder::Attack { target: target.id }))
    }
}
