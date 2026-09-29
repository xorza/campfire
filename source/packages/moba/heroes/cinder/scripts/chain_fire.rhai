fn on_cast(ctx, caster, target) {
    ctx.projectile_homing(caster, caster, target, #{ speed: ctx.value("speed") });
}

fn on_projectile_hit(ctx, proj, target) {
    ctx.damage(proj.source, target, ctx.value("damage"), "magic");
    ctx.add_modifier(target, "kindle");
    if proj.state.bounces_left == 0 {
        return;
    }
    let heroes = [];
    let others = [];
    for unit in ctx.visible_enemies_near(proj.source, target.pos, ctx.value("bounce_radius")) {
        if unit == target {
            continue;
        }
        if unit.is_hero {
            heroes.push(unit);
        } else {
            others.push(unit);
        }
    }
    let pool = if heroes.is_empty() { others } else { heroes };
    if pool.is_empty() {
        return;
    }
    let next = ctx.projectile_homing(proj.source, target, ctx.pick(pool), #{ speed: ctx.value("speed") });
    next.state.bounces_left = proj.state.bounces_left - 1;
}
