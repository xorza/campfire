fn on_cast(ctx, caster, target) {
    let m = ctx.add_modifier(caster, "flicker_strike");
    m.state.left = ctx.value("strikes");
    strike(ctx, m, caster, target);
}

fn on_interval(ctx, m) {
    let options = [];
    for unit in ctx.visible_enemies_near(m.carrier, m.state.last.pos, ctx.value("hop_radius")) {
        if unit.targetable && !m.state.struck.contains(unit) {
            options.push(unit);
        }
    }
    if m.state.left == 0 || options.is_empty() {
        ctx.remove_modifier(m.carrier, "flicker_strike");
        return;
    }
    strike(ctx, m, m.carrier, ctx.pick(options));
}

fn strike(ctx, m, caster, target) {
    let damage = ctx.value("damage");
    if !target.is_hero && ctx.chance(ctx.value("minion_bonus_chance")) {
        damage += ctx.value("minion_bonus");
    }
    ctx.teleport(caster, target.pos);
    ctx.damage(caster, target, damage, "physical");
    m.state.last = target;
    m.state.struck.push(target);
    m.state.left -= 1;
}
