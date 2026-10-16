use campfire_common::Ticks;
use campfire_script::{
    ApiBuilder, ApiError, ApiOwner, ApiVersion, Applies, Checked, Ctx, DataTable, Hook, MemberSpec,
    Status, Unit,
};
use campfire_values::{ModifierId, Rank, TagProperty};

use crate::meter::Meter;
use crate::modifier_data::{AuraData, ModifierData};
use crate::modifier_handle::ModifierHandle;
use crate::pool_id::PoolId;
use crate::stat_id::StatId;
use crate::stats_call::StatsCall;
use crate::stats_column::StatsColumn;
use crate::stats_effect::StatsEffect;

/// The script API of `stats`: `ctx.add_modifier`, `ctx.remove`, the `Modifier` handle, and the
/// planned crowd control.
#[derive(Debug)]
pub struct StatsApi;

impl StatsApi {
    /// What a unit's stats give it: its level, its stats, its pools and the modifiers it
    /// carries.
    fn register_unit(api: &mut ApiBuilder<'_>) {
        let field = |name, description| MemberSpec::field(ApiOwner::Unit, name, description);
        let method = |name, signature, description| {
            MemberSpec::method(ApiOwner::Unit, name, signature, description)
        };
        api.bind(
            field("level", "its level"),
            |unit: &mut Unit| -> Checked<i64> {
                let level = StatsColumn::level(unit.view(), unit.row_index())?;
                Ok(i64::from(level))
            },
        )
        .method(
            method(
                "stat",
                &[&["name"]],
                "its value of a stat the mode declares",
            ),
            |unit: Unit, stat: StatId| StatsColumn::stat(unit.view(), unit.row_index(), stat),
        )
        .method(
            method(
                "pool",
                &[&["name"]],
                "the current amount of its pool `name`",
            ),
            |unit: Unit, pool: PoolId| {
                StatsColumn::pool(unit.view(), unit.row_index(), pool, Meter::current)
            },
        )
        .method(
            method("pool_max", &[&["name"]], "the maximum of its pool `name`"),
            |unit: Unit, pool: PoolId| {
                StatsColumn::pool(unit.view(), unit.row_index(), pool, Meter::max)
            },
        )
        .method(
            method(
                "has_modifier",
                &[&["id"]],
                "whether it carries the modifier of the script's package",
            ),
            |unit: Unit, id: ModifierId| {
                StatsColumn::has_modifier(unit.view(), unit.row_index(), id)
            },
        );
    }

    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        ModifierHandle::register(api);
        Self::register_unit(api);
        let call = |name, signature, description| MemberSpec::call(name, signature, description);
        let add = call(
            "add_modifier",
            &[&["unit", "id"], &["unit", "id", "duration_ms"]],
            "applies the modifier `id` of the script's package to `unit` from the acting unit, with \
             the call's action at its rank, which gives each param the modifier reads and does not \
             declare, and returns its handle",
        )
        .applies(Applies::WithAction);
        api.call(add, |ctx: &mut Ctx, target: Unit, id: ModifierId| {
            Self::add_modifier(ctx, &target, id, None)
        })
        .call(
            add,
            |ctx: &mut Ctx, target: Unit, id: ModifierId, ms: i64| {
                let ticks = ctx.view().ticks(ms).map_err(ApiError::fail)?;
                Self::add_modifier(ctx, &target, id, Some(ticks))
            },
        )
        .call(
            call(
                "add_player_modifier",
                &[&["player", "id"]],
                "gives `player` the modifier `id` of the script's package, which every living unit \
                 it owns that the modifier's `affects` selects holds from no source and with no \
                 action, so the modifier declares each param it reads",
            )
            .applies(Applies::WithoutAction),
            |ctx: &mut Ctx, player: i64, id: ModifierId| Self::add_player_modifier(ctx, player, id),
        )
        .call(
            call(
                "remove",
                &[&["handle"]],
                "ends the modifier, projectile or area at once",
            ),
            |ctx: &mut Ctx, handle: ModifierHandle| ctx.queue(handle.remove()),
        );
        api.hook(Hook::OnInterval, Status::Runs(ApiVersion::FIRST))
            .tag_property(TagProperty::Immune, Status::Runs(ApiVersion::FIRST));
        api.data::<ModifierData>(
            &[
                "script",
                "duration_ms",
                "interval_ms",
                "stacks_expire_ms",
                "reapply",
                "max_stacks",
                "stats",
                "shield",
                "aura",
                "affects",
                "params",
                "state",
                "tags",
            ],
            &[],
        )
        .data::<AuraData>(&["radius", "affects", "modifier"], &[])
        .shared_data(DataTable::Mode, &["stats"], &[])
        .shared_data(DataTable::UnitType, &["stats", "pools", "passive"], &[]);
    }

    /// Queues modifier `id` of the call's package for `player`, one of the session's; an error
    /// when `id` reads a param it does not declare.
    fn add_player_modifier(ctx: &Ctx, player: i64, id: ModifierId) -> Checked<()> {
        let player = ctx.view().names().player(player)?;
        StatsColumn::check_way(
            ctx.view(),
            StatsCall::of(&ctx.frame()),
            id,
            None,
            Rank::FIRST,
        )?;
        ctx.queue(StatsEffect::AddPlayer { player, id })
    }

    /// Queues modifier `id` of the call's package on `target`, from the acting unit, for
    /// `duration` when given, and gives its handle; an error when the call's action at its rank
    /// does not give a param `id` reads.
    fn add_modifier(
        ctx: &Ctx,
        target: &Unit,
        id: ModifierId,
        duration: Option<Ticks>,
    ) -> Checked<ModifierHandle> {
        let mut frame = ctx.write()?;
        let (source, action, rank) = (frame.acting(), frame.action(), frame.rank());
        let call = StatsCall::of_mut(&mut frame);
        StatsColumn::check_way(ctx.view(), call, id, action, rank)?;
        let handle = StatsColumn::applied_handle(ctx.view(), call, target.id, id, source);
        frame.queue(StatsEffect::Add {
            target: target.id,
            id,
            duration,
        });
        Ok(handle)
    }
}
