fn on_channel_tick(ctx, caster) {
    ctx.heal(caster, ctx.p.heal_per_tick);
}
