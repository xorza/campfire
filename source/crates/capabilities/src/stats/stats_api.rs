use campfire_script::rhai::INT;
use campfire_sim::{Capability, Ticks};

use crate::mode::mode_api::ModeApi;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::effect::Effect;
use crate::scripts::error::Checked;
use crate::scripts::hook::Hook;
use crate::scripts::script_api::{DataTable, MemberSpec, Status};
use crate::stats::modifier_effect::ModifierEffect;
use crate::stats::modifier_handle::ModifierHandle;
use crate::units::tag_effect::TagEffect;
use crate::units::unit::Unit;

/// The script API of `stats`: `ctx.add_modifier`, `ctx.remove`, the `Modifier` handle, and the
/// planned crowd control and experience.
#[derive(Debug)]
pub(crate) struct StatsApi;

impl StatsApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        ModifierHandle::register(api);
        let call = |name, signature, description| {
            MemberSpec::call(name, signature, description).capability(Capability::Stats)
        };
        let add = call(
            "add_modifier",
            "(unit, id) or (unit, id, duration_ms)",
            "applies the modifier `id` of the script's package to `unit` from the acting unit, and returns its handle",
        );
        api.bind(add, |ctx: &mut Ctx, target: Unit, id: &str| {
            StatsApi::add_modifier(ctx, &target, id, None)
        })
        .bind(add, |ctx: &mut Ctx, target: Unit, id: &str, ms: INT| {
            let ticks = ctx.view().ticks(ms)?;
            StatsApi::add_modifier(ctx, &target, id, Some(ticks))
        })
        .bind(
            call(
                "add_player_modifier",
                "(player, id)",
                "gives `player` the modifier `id` of the script's package, which every living unit \
                 it owns that the modifier's `affects` selects holds from no source",
            ),
            |ctx: &mut Ctx, player: INT, id: &str| StatsApi::add_player_modifier(ctx, player, id),
        )
        .bind(
            call(
                "remove",
                "(handle)",
                "ends the modifier, projectile or area at once",
            ),
            |ctx: &mut Ctx, handle: ModifierHandle| ctx.queue(Effect::Modifier(handle.remove())),
        )
        .plan(call(
            "knock_back",
            "(unit, from, distance, ms)",
            "pushes `unit` away from `from`",
        ))
        .plan(call(
            "add_xp",
            "(avatar, amount)",
            "gives `avatar` experience",
        ));
        api.hook(Hook::OnInterval, "(ctx, m)", Status::Runs)
            .hook(Hook::OnAttack, "(ctx, m, target)", Status::Runs)
            .hook(Hook::OnAttackHit, "(ctx, m, d)", Status::Runs)
            .hook(Hook::OnDamageTaken, "(ctx, m, d)", Status::Runs)
            .hook(Hook::OnKill, "(ctx, m, victim)", Status::Runs)
            .hook(Hook::OnTakedown, "(ctx, m, victim)", Status::Runs)
            .tag_effect(TagEffect::Immune, Status::Runs);
        api.data(
            DataTable::Modifier,
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
        .data(DataTable::Aura, &["radius", "affects", "modifier"], &[]);
    }

    /// Queues modifier `id` of the call's package for `player`, one of the session's.
    fn add_player_modifier(ctx: &Ctx, player: INT, id: &str) -> Checked<()> {
        let player = ModeApi::player(ctx.mode_or_fail()?, player)?;
        let id = ctx.view().modifier(id)?;
        ctx.queue(Effect::Modifier(ModifierEffect::AddPlayer { player, id }))
    }

    /// Queues modifier `id` of the call's package on `target`, from the acting unit, for
    /// `duration` when given, and gives its handle.
    fn add_modifier(
        ctx: &Ctx,
        target: &Unit,
        id: &str,
        duration: Option<Ticks>,
    ) -> Checked<ModifierHandle> {
        let id = ctx.view().modifier(id)?;
        let mut frame = ctx.write()?;
        let source = frame.acting();
        let handle = ctx
            .view()
            .applied_handle(&mut frame.handles, target.id, id, source);
        frame.effects.push(Effect::Modifier(ModifierEffect::Add {
            target: target.id,
            id,
            duration,
        }));
        Ok(handle)
    }
}
