use campfire_script::rhai::ImmutableString;
use campfire_sim::Capability;

use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::{Ctx, Params};
use crate::scripts::script_api::{DataTable, MemberSpec, Status};
use crate::units::block::Block;
use crate::units::tag_effect::TagEffect;

/// The script API of the core: `ctx` itself, `ctx.p`, and the core's and the movement calls
/// design 08 plans.
#[derive(Debug)]
pub(crate) struct CoreApi;

impl CoreApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.ty::<Ctx>("Ctx")
            .bind(
                MemberSpec::value(
                    "p",
                    "the params: an ability's at its rank, a modifier's then its ability's, or the mode's",
                ),
                |ctx: &mut Ctx| Params(ctx.clone()),
            )
            .plan(MemberSpec::call(
                "chance",
                "(p)",
                "true with probability `p`, from the secret stream",
            ))
            .plan(MemberSpec::call(
                "pick",
                "(list)",
                "an entry of `list`, from the secret stream",
            ))
            .plan(
                MemberSpec::call("dash", "(unit, to, speed)", "moves `unit` to `to` at `speed`")
                    .capability(Capability::Navigation),
            )
            .plan(
                MemberSpec::call("teleport", "(unit, pos)", "puts `unit` at `pos`")
                    .capability(Capability::Navigation),
            )
            .data(DataTable::ModeNavigation, &["layers"], &[])
            .data(DataTable::Collision, &["radius", "layer"], &[])
            .data(DataTable::Tag, &["blocks", "hidden", "detects", "immune"], &[])
            .data(
                DataTable::Action,
                &[
                    "kind",
                    "targeting",
                    "range",
                    "cost",
                    "windup_ms",
                    "passive_modifier",
                    "passive_while_ready",
                ],
                &[],
            );
        api.tag_effect(TagEffect::Blocks(Block::Move), Status::Runs)
            .tag_effect(TagEffect::Blocks(Block::Use), Status::Planned);
        api.ty::<Params>("Params")
            .index(|params: &mut Params, name: ImmutableString| params.get(&name));
    }
}
