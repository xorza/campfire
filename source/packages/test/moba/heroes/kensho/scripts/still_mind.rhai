fn on_channel_tick(ctx, caster, target) {
    ctx.heal(caster, ctx.params.heal_per_tick);
}
