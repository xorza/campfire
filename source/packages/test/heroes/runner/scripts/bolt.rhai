fn on_cast(ctx, caster, target) {
    ctx.damage(target, ctx.p.damage, "true");
}
