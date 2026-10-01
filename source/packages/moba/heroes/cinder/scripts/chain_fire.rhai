fn on_resolve(ctx, caster, target) {
    ctx.projectile(caster, target);
}

fn on_hit(ctx, caster, target, hit) {
    ctx.damage(target, ctx.p.damage, "magic");
    ctx.add_modifier(target, "kindle");
    if hit.delivery.state.bounces_left == 0 {
        return;
    }
    let pool = without(ctx.find_visible(caster, target.pos, ctx.p.bounce_radius, "enemies:avatar"), target);
    if pool.is_empty() {
        pool = without(ctx.find_visible(caster, target.pos, ctx.p.bounce_radius, "enemies"), target);
    }
    if !pool.is_empty() {
        let next = ctx.projectile(target, ctx.pick(pool));
        next.state.bounces_left = hit.delivery.state.bounces_left - 1;
    }
}

fn without(units, excluded) {
    let kept = [];
    for unit in units {
        if unit != excluded {
            kept.push(unit);
        }
    }
    kept
}
