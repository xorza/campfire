fn on_cast(ctx, caster, target) {
    ctx.add_modifier(caster, "still_mind");
}

fn on_channel_tick(ctx, caster) {
    ctx.heal(caster, ctx.value("heal_per_tick"));
}

fn on_channel_end(ctx, caster, interrupted) {
    ctx.remove_modifier(caster, "still_mind");
}
