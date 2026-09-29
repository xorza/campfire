fn on_cast(ctx, caster, target) {
    let m = ctx.add_modifier(caster, "flicker_strike");
    m.state.left = ctx.p.strikes;
    strike(ctx, m, target);
}

fn on_interval(ctx, m) {
    let options = [];
    for unit in ctx.find_visible(m.carrier, m.state.last.pos, ctx.p.hop_radius, "enemies") {
        if !m.state.struck.contains(unit) {
            options.push(unit);
        }
    }
    if options.is_empty() {
        ctx.remove(m);
    } else {
        strike(ctx, m, ctx.pick(options));
    }
}

fn strike(ctx, m, target) {
    let damage = ctx.p.damage;
    if !target.is_hero && ctx.chance(ctx.p.minion_bonus_chance) {
        damage += ctx.p.minion_bonus;
    }
    ctx.teleport(m.carrier, target.pos);
    ctx.damage(target, damage, "physical");
    m.state.last = target;
    m.state.struck.push(target);
    m.state.left -= 1;
    if m.state.left == 0 {
        ctx.remove(m);
    }
}
