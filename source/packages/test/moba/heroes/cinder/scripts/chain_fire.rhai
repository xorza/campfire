fn on_hit(ctx, caster, target, hit) {
    ctx.damage(target, "magic", ctx.params.damage);
    ctx.add_modifier(target, "kindle");
    if hit.delivery.state.bounces_left == 0 {
        return;
    }
    let pool = without(ctx.find_visible(caster, target.position, ctx.params.bounce_radius, "enemies:avatar"), target);
    if pool.is_empty() {
        pool = without(ctx.find_visible(caster, target.position, ctx.params.bounce_radius, "enemies"), target);
    }
    if !pool.is_empty() {
        let next = ctx.launch(target.position, ctx.pick(pool));
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
