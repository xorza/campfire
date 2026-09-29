fn on_cast(ctx, caster, target) {
    let arrows = ctx.value("arrows");
    let step = ctx.value("cone_deg") / (arrows - 1);
    let half = ctx.value("cone_deg") / 2;
    let aim = caster.pos.direction_to(target);
    for i in 0..arrows {
        ctx.projectile_line(caster, caster.pos, aim.rotated_deg(step * i - half), #{
            speed: ctx.value("speed"),
            range: ctx.range,
            width: ctx.value("width"),
            stop_on_hit: true,
            once_per_cast: true,
        });
    }
}

fn on_projectile_hit(ctx, proj, target) {
    ctx.damage(proj.source, target, ctx.value("damage"), "physical");
    ctx.slow(target, ctx.value("slow"), ctx.value("slow_ms"));
}
