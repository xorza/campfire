fn on_cast(ctx, caster, target) {
    ctx.reveal(caster, target, ctx.value("radius"), ctx.value("reveal_ms"));
}
