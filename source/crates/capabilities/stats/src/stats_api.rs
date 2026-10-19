use campfire_common::PlayerSlot;
use campfire_script::{
    ApiBuilder, ApiOwner, ApiVersion, Applies, Checked, Ctx, MemberSpec, ScriptValue, Status, Unit,
};
use campfire_units::{Meter, PoolId, StatId};
use campfire_values::{ModifierId, Rank, TagProperty};

use crate::modifier_data::{AuraData, ModifierData};
use crate::modifier_handle::ModifierHandle;
use crate::stats_call::StatsCall;
use crate::stats_column::{Carried, CarriedPick, StatsColumn};
use crate::stats_effect::StatsEffect;

/// The script API of `stats` beside its effects' calls: `ctx.add_player_modifier`, `ctx.remove`,
/// the `Modifier` handle, and the planned crowd control.
#[derive(Debug)]
pub struct StatsApi;

impl StatsApi {
    /// What a unit's stats give it: its level, its stats, its pools and the modifiers it
    /// carries.
    fn register_unit(api: &mut ApiBuilder<'_>) {
        let field =
            |name, description| MemberSpec::field(ApiOwner::of::<Unit>(), name, description);
        let method = |name, signature, description| {
            MemberSpec::method(ApiOwner::of::<Unit>(), name, signature, description)
        };
        api.bind(
            field("level", "its level"),
            |unit: &mut Unit| -> Checked<i64> {
                let level = StatsColumn::level(unit.view(), unit.row_index()?)?;
                Ok(i64::from(level))
            },
        )
        .method(
            method(
                "stat",
                &[&["name"]],
                "its value of a stat the mode declares",
            ),
            |unit: Unit, stat: StatId| StatsColumn::stat(unit.view(), unit.row_index()?, stat),
        )
        .method(
            method(
                "pool",
                &[&["name"]],
                "the current amount of its pool `name`",
            ),
            |unit: Unit, pool: PoolId| {
                StatsColumn::pool(unit.view(), unit.row_index()?, pool, Meter::current)
            },
        )
        .method(
            method("pool_max", &[&["name"]], "the maximum of its pool `name`"),
            |unit: Unit, pool: PoolId| {
                StatsColumn::pool(unit.view(), unit.row_index()?, pool, Meter::max)
            },
        )
        .method(
            method(
                "has_modifier",
                &[&["id"]],
                "whether it carries the modifier of the script's package",
            ),
            |unit: Unit, id: ModifierId| {
                StatsColumn::has_modifier(unit.view(), unit.row_index()?, id)
            },
        );
        let modifier = method(
            "modifier",
            &[&["id"], &["id", "source"]],
            "its instance of the modifier `id` of the script's package, to read or remove: the one from `source`, or the first, from no source before any, then by source; `()` with none",
        );
        api.method(modifier, |ctx: &Ctx, unit: Unit, id: ModifierId| {
            Self::modifier(ctx, &unit, id, CarriedPick::First)
        })
        .method(
            modifier,
            |ctx: &Ctx, unit: Unit, id: ModifierId, source: Unit| {
                Self::modifier(ctx, &unit, id, CarriedPick::From(source.id))
            },
        );
    }

    /// `unit.modifier`: the handle of `unit`'s instance of `id` that `pick` picks, or `()`.
    fn modifier(ctx: &Ctx, unit: &Unit, id: ModifierId, pick: CarriedPick) -> Checked<ScriptValue> {
        let carried = Carried {
            carrier: unit.id,
            row: unit.row_index()?,
            id,
            pick,
        };
        let mut frame = ctx.frame();
        let call = StatsCall::of_mut(&mut frame);
        let handle = StatsColumn::carried_handle(ctx.view(), call, carried)?;
        Ok(handle.map_or(ScriptValue::UNIT, ScriptValue::of))
    }

    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        ModifierHandle::register(api);
        Self::register_unit(api);
        let call = |name, signature, description| MemberSpec::call(name, signature, description);
        api.call(
            call(
                "add_player_modifier",
                &[&["player", "id"]],
                "gives `player` the modifier `id` of the script's package, which every living unit \
                 it owns that the modifier's `affects` selects holds from no source and with no \
                 action, so the modifier declares each param it reads",
            )
            .applies(Applies::WithoutAction),
            |ctx: &mut Ctx, player: PlayerSlot, id: ModifierId| {
                Self::add_player_modifier(ctx, player, id)
            },
        )
        .call(
            call(
                "remove_player_modifier",
                &[&["player", "id"]],
                "takes the modifier `id` of the script's package from `player`, and so from each \
                 unit it owns, if it holds it",
            ),
            |ctx: &mut Ctx, player: PlayerSlot, id: ModifierId| {
                ctx.queue(StatsEffect::RemovePlayer { player, id })
            },
        )
        .call(
            call(
                "remove_modifier",
                &[&["m"]],
                "ends the modifier of `m` at once",
            ),
            |ctx: &mut Ctx, handle: ModifierHandle| ctx.queue(StatsEffect::removing(&handle)),
        );
        api.tag_property(TagProperty::Immune, Status::Runs(ApiVersion::FIRST));
        api.data::<ModifierData>(&[]).data::<AuraData>(&[]);
    }

    /// Queues modifier `id` of the call's package for `player`, one of the session's; an error
    /// when `id` reads a param it does not declare.
    fn add_player_modifier(ctx: &Ctx, player: PlayerSlot, id: ModifierId) -> Checked<()> {
        StatsColumn::check_way(
            ctx.view(),
            StatsCall::of(&ctx.frame()),
            id,
            None,
            Rank::FIRST,
        )?;
        ctx.queue(StatsEffect::AddPlayer { player, id })
    }
}
