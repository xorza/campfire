fn on_cast(ctx, caster, target) {
    ctx.damage(caster, target, ctx.value("damage"), "magic");
    ctx.slow(target, ctx.value("slow"), ctx.value("slow_ms"));
}
