use campfire_math::Num;
use campfire_script::rhai::{Dynamic, INT};
use campfire_sim::{Capability, Position};

use crate::abilities::abilities_effect::AbilitiesEffect;
use crate::actions::actions_column::ActionsColumn;
use crate::actions::range::Range;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::name_kind::NameKind;
use crate::units::action_id::ActionId;
use crate::units::unit::Unit;

use crate::actions::action_data_field::ActionDataField;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::hook::Hook;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::scripts::script_api::status::Status;
use crate::units::block::Block;
use crate::units::tag_property::TagProperty;

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
        api.bind_for(
            cast(
                "range",
                "the ability's range at its rank in meters, `()` for a global one",
            ),
            |ctx: &mut Ctx| Ok(AbilitiesApi::range(ctx)),
        )
        .bind_for(
            cast(
                "charge",
                "the share of its most a charged action charged, from 0 to 1",
            ),
            |ctx: &mut Ctx| AbilitiesApi::charge(ctx),
        )
        .bind_for(
            cast("origin", "where the action's unit stood as it started"),
            |ctx: &mut Ctx| AbilitiesApi::origin(ctx),
        )
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
        .bind(
            call(
                "add_charge",
                "(unit, id)",
                "gives `unit`'s ability `id`, of the script's package, a charge, up to its most",
            )
            .name(1, NameKind::Ability),
            |ctx: &mut Ctx, unit: Unit, id: &str| AbilitiesApi::add_charge(ctx, &unit, id),
        );
        api.tag_property(
            TagProperty::Blocks(Block::Cast),
            Status::Runs(ApiVersion::FIRST),
        )
        .hook(Hook::OnResolve, Status::Runs(ApiVersion::FIRST))
        .hook(Hook::OnChannelTick, Status::Runs(ApiVersion::FIRST))
        .hook(Hook::OnInterrupt, Status::Runs(ApiVersion::FIRST))
        .action_fields(ActionDataField::of(Some(Capability::Abilities)));
    }
    /// Where the running call's action's unit stood as the action started.
    fn origin(ctx: &Ctx) -> Checked<Position> {
        let start = ctx.frame().start();
        Ok(start.ok_or_else(|| ApiError::NoStart.fail())?.origin)
    }

    /// The share of its most the running call's charged action charged.
    fn charge(ctx: &Ctx) -> Checked<Num> {
        let start = ctx.frame().start();
        let start = start.ok_or_else(|| ApiError::NoStart.fail())?;
        Ok(start.charge.ok_or_else(|| ApiError::NotCharged.fail())?)
    }

    /// The range of the running call's action at its rank: meters, or `()` for a global one.
    fn range(ctx: &Ctx) -> Dynamic {
        let (action, rank) = {
            let frame = ctx.frame();
            let action = frame.action().expect("an action's call has its action");
            (action, frame.rank())
        };
        match ActionsColumn::range(ctx.view(), action, rank) {
            Range::Meters(meters) => Dynamic::from(meters),
            Range::Global => Dynamic::UNIT,
        }
    }

    /// Queues `ms`, rounded up to ticks, off the cooldown of `unit`'s action `id` of the script's
    /// package, which it holds.
    fn reduce_cooldown(ctx: &Ctx, unit: &Unit, id: &str, ms: INT) -> Checked<()> {
        let action = AbilitiesApi::held(ctx, unit, id)?;
        let cut = ctx.view().ticks(ms)?;
        ctx.queue(AbilitiesEffect::ReduceCooldown {
            unit: unit.id,
            action,
            cut,
        })
    }

    /// Queues a charge more of `unit`'s action `id` of the script's package, which it holds and
    /// which has charges.
    fn add_charge(ctx: &Ctx, unit: &Unit, id: &str) -> Checked<()> {
        let action = AbilitiesApi::held(ctx, unit, id)?;
        if !ActionsColumn::has_charges(ctx.view(), action) {
            return Err(ApiError::NoCharges.fail().into());
        }
        ctx.queue(AbilitiesEffect::AddCharge {
            unit: unit.id,
            action,
        })
    }

    /// The action `id` of the script's package, which `unit` holds.
    fn held(ctx: &Ctx, unit: &Unit, id: &str) -> Checked<ActionId> {
        let view = ctx.view();
        let action = ActionsColumn::action_named(view, ctx.frame().package(), id)?;
        if !ActionsColumn::holds(view, unit.row_index(), action) {
            return Err(ApiError::NotHeld.fail().into());
        }
        Ok(action)
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
