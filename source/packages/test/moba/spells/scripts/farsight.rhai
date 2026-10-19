fn on_resolve(ctx, caster, target) {
    ctx.reveal(target, ctx.params.radius, ctx.params.reveal_ms);
}
