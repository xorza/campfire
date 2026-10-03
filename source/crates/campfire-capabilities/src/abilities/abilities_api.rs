use campfire_math::Num;
use campfire_script::rhai::{Dynamic, INT};
use campfire_sim::Capability;

use crate::abilities::abilities_effect::AbilitiesEffect;
use crate::actions::actions_column::ActionsColumn;
use crate::actions::range::Range;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::name_kind::NameKind;
use crate::units::unit::Unit;

use crate::actions::action_data_field::ActionDataField;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::hook::Hook;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::scripts::script_api::status::Status;
use crate::units::block::Block;
use crate::units::tag_effect::TagEffect;

/// The script API of `abilities`: the values of a cast that design 08 plans, and the
/// bookkeeping of cooldowns and charges.
#[derive(Debug)]
pub(crate) struct AbilitiesApi;

impl AbilitiesApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let cast = |name, description| {
            MemberSpec::value(name, description)
                .roles(RoleSet::ACTION)
                .capability(Capability::Abilities)
        };
        let call = |name, signature, description| {
            MemberSpec::call(name, signature, description).capability(Capability::Abilities)
        };
        let cut = call(
            "reduce_cooldowns",
            "(unit, kind, fraction)",
            "takes `fraction` of what is left off the cooldowns of `unit`'s abilities in the slot kind `kind`",
        )
        .name(1, NameKind::SlotKind);
        api.bind(
            cast(
                "range",
                "the ability's range at its rank in meters, `()` for a global one",
            ),
            |ctx: &mut Ctx| AbilitiesApi::range(ctx),
        )
        .plan(cast(
            "charge",
            "how long a charged cast was held, from 0 to 1",
        ))
        .plan(cast("origin", "where the cast's unit stood as it started"))
        .bind(
            call(
                "reduce_cooldown",
                "(unit, id, ms)",
                "takes `ms` off the cooldown of `unit`'s ability `id`, of the script's package",
            )
            .name(1, NameKind::Ability),
            |ctx: &mut Ctx, unit: Unit, id: &str, ms: INT| {
                AbilitiesApi::reduce_cooldown(ctx, &unit, id, ms)
            },
        )
        .bind(
            cut,
            |ctx: &mut Ctx, unit: Unit, kind: &str, fraction: Num| {
                AbilitiesApi::reduce_cooldowns(ctx, &unit, kind, fraction)
            },
        )
        .bind(
            cut,
            |ctx: &mut Ctx, unit: Unit, kind: &str, fraction: INT| {
                AbilitiesApi::reduce_cooldowns(ctx, &unit, kind, ApiError::num(fraction)?)
            },
        )
        .plan(call(
            "add_charge",
            "(unit, id)",
            "gives `unit`'s ability `id` a charge",
        ));
        api.tag_effect(
            TagEffect::Blocks(Block::Cast),
            Status::Runs(ApiVersion::FIRST),
        )
        .hook(Hook::OnResolve, Status::Runs(ApiVersion::FIRST))
        .hook(Hook::OnChannelTick, Status::Planned)
        .hook(Hook::OnInterrupt, Status::Planned)
        .action_fields(ActionDataField::of(Some(Capability::Abilities)));
    }
    /// The range of the running call's action at its rank: meters, or `()` for a global one.
    fn range(ctx: &Ctx) -> Checked<Dynamic> {
        ctx.require(RoleSet::ACTION)?;
        let (action, rank) = {
            let frame = ctx.frame();
            let action = frame.action().expect("an action's call has its action");
            (action, frame.rank())
        };
        Ok(match ActionsColumn::range(ctx.view(), action, rank) {
            Range::Meters(meters) => Dynamic::from(meters),
            Range::Global => Dynamic::UNIT,
        })
    }

    /// Queues `ms`, rounded up to ticks, off the cooldown of `unit`'s action `id` of the script's
    /// package, which it holds.
    fn reduce_cooldown(ctx: &Ctx, unit: &Unit, id: &str, ms: INT) -> Checked<()> {
        let view = ctx.view();
        let package = ctx.frame().package();
        let action = ActionsColumn::action_named(view, package, id)?;
        if !ActionsColumn::holds(view, unit.row_index(), action) {
            return Err(ApiError::NotHeld.fail().into());
        }
        let cut = view.ticks(ms)?;
        ctx.queue(AbilitiesEffect::ReduceCooldown {
            unit: unit.id,
            action,
            cut,
        })
    }

    /// Queues `fraction` of what is left off the cooldowns of `unit`'s actions in `kind`.
    fn reduce_cooldowns(ctx: &Ctx, unit: &Unit, kind: &str, fraction: Num) -> Checked<()> {
        let kind = ActionsColumn::kind_named(ctx.view(), kind)?;
        if !(Num::ZERO..=Num::ONE).contains(&fraction) {
            return Err(ApiError::NotAFraction.fail().into());
        }
        ctx.queue(AbilitiesEffect::ReduceCooldowns {
            unit: unit.id,
            kind,
            fraction,
        })
    }
}
