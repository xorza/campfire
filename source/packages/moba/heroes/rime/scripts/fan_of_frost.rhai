fn on_resolve(ctx, caster, target) {
    let arrows = ctx.p.arrows;
    let spread = if arrows > 1 { ctx.p.cone_deg / (arrows - 1) } else { num(0) };
    let first = spread * (arrows - 1) / 2;
    let aim = caster.pos.direction_to(target);
    for i in 0..arrows {
        ctx.projectile(caster.pos, aim.rotated_deg(spread * i - first));
    }
}

fn on_hit(ctx, caster, target, hit) {
    ctx.damage(target, ctx.p.damage, "physical");
    ctx.slow(target, ctx.p.slow, ctx.p.slow_ms);
}
