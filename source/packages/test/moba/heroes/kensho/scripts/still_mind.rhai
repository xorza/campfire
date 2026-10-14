fn on_channel_tick(ctx, caster, target) {
    ctx.heal(caster, ctx.p.heal_per_tick);
}
