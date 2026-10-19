use campfire_actions::{ActionRange, ActionsColumn, SlotKind};
use campfire_math::{Num, Vec3};
use campfire_script::{
    ApiBuilder, ApiVersion, CapabilityRefusal, Checked, Ctx, MemberSpec, RoleSet, ScriptCut,
    ScriptValue, Status, Unit,
};
use campfire_values::{ActionId, Block, TagProperty};

use crate::abilities_effect::AbilitiesEffect;
use crate::error::AbilitiesRefusal;

/// The script API of `abilities`: the values of a cast that the script API plans, and the
/// bookkeeping of cooldowns and charges.
#[derive(Debug)]
pub struct AbilitiesApi;

impl AbilitiesApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let cast = |name, description| MemberSpec::value(name, description).roles(RoleSet::ACTION);
        let call = |name, signature, description| MemberSpec::call(name, signature, description);
        let cut = call(
            "scale_cooldowns",
            &[&["unit", "kind", "fraction"]],
            "takes `fraction` of what is left off the cooldowns of `unit`'s abilities in the slot kind `kind`",
        );
        api.call(
            cast(
                "range",
                "the ability's range at its rank in meters, `()` for a global one",
            ),
            |ctx: &mut Ctx| Ok(Self::range(ctx)),
        )
        .call(
            cast(
                "charged",
                "the share of its most a charged action charged, from 0 to 1",
            ),
            |ctx: &mut Ctx| Self::charge(ctx),
        )
        .call(
            cast("origin", "where the action's unit stood as it started"),
            |ctx: &mut Ctx| Self::origin(ctx),
        )
        .call(
            call(
                "cut_cooldown",
                &[&["unit", "action", "ms"]],
                "takes `ms` off the cooldown of `unit`'s action `action`, of the script's package",
            ),
            |ctx: &mut Ctx, unit: Unit, id: ActionId, ms: ScriptCut| {
                Self::reduce_cooldown(ctx, &unit, id, ms)
            },
        )
        .call(
            cut,
            |ctx: &mut Ctx, unit: Unit, kind: SlotKind, fraction: Num| {
                Self::reduce_cooldowns(ctx, &unit, kind, fraction)
            },
        )
        .call(
            call(
                "add_charge",
                &[&["unit", "id"]],
                "gives `unit`'s ability `id`, of the script's package, a charge, up to its most",
            ),
            |ctx: &mut Ctx, unit: Unit, id: ActionId| Self::add_charge(ctx, &unit, id),
        );
        api.tag_property(
            TagProperty::Blocks(Block::Cast),
            Status::Runs(ApiVersion::FIRST),
        );
    }

    /// Where the running call's action's unit stood as the action started.
    fn origin(ctx: &Ctx) -> Checked<Vec3> {
        let start = ctx.frame().start();
        Ok(start
            .ok_or_else(|| AbilitiesRefusal::NoStart.fail())?
            .origin)
    }

    /// The share of its most the running call's charged action charged.
    fn charge(ctx: &Ctx) -> Checked<Num> {
        let start = ctx.frame().start();
        let start = start.ok_or_else(|| AbilitiesRefusal::NoStart.fail())?;
        Ok(start
            .charge
            .ok_or_else(|| AbilitiesRefusal::NotCharged.fail())?)
    }

    /// The range of the running call's action at its rank: meters, or `()` for a global one.
    fn range(ctx: &Ctx) -> ScriptValue {
        let (action, rank) = {
            let frame = ctx.frame();
            let action = frame.action().expect("an action's call has its action");
            (action, frame.rank())
        };
        match ActionsColumn::range(ctx.view(), action, rank) {
            ActionRange::Meters(meters) => ScriptValue::of(meters.get()),
            ActionRange::Global => ScriptValue::UNIT,
        }
    }

    /// Queues `ms` off the cooldown of `unit`'s action `action` of the script's package, which
    /// it holds.
    fn reduce_cooldown(ctx: &Ctx, unit: &Unit, action: ActionId, ms: ScriptCut) -> Checked<()> {
        Self::check_held(ctx, unit, action)?;
        let cut = ms.get();
        ctx.queue(AbilitiesEffect::ReduceCooldown {
            unit: unit.id,
            action,
            cut,
        })
    }

    /// Queues a charge more of `unit`'s action `action` of the script's package, which it holds and
    /// which has charges.
    fn add_charge(ctx: &Ctx, unit: &Unit, action: ActionId) -> Checked<()> {
        Self::check_held(ctx, unit, action)?;
        if !ActionsColumn::has_charges(ctx.view(), action) {
            return Err(AbilitiesRefusal::NoCharges.fail().into());
        }
        ctx.queue(AbilitiesEffect::AddCharge {
            unit: unit.id,
            action,
        })
    }

    /// Refuses `action` unless `unit` holds it.
    fn check_held(ctx: &Ctx, unit: &Unit, action: ActionId) -> Checked<()> {
        if !ActionsColumn::holds(ctx.view(), unit.row_index()?, action) {
            return Err(AbilitiesRefusal::NotHeld.fail().into());
        }
        Ok(())
    }

    /// Queues `fraction` of what is left off the cooldowns of `unit`'s actions in `kind`.
    fn reduce_cooldowns(ctx: &Ctx, unit: &Unit, kind: SlotKind, fraction: Num) -> Checked<()> {
        if !(Num::ZERO..=Num::ONE).contains(&fraction) {
            return Err(AbilitiesRefusal::NotAFraction.fail().into());
        }
        ctx.queue(AbilitiesEffect::ReduceCooldowns {
            unit: unit.id,
            kind,
            fraction,
        })
    }
}
