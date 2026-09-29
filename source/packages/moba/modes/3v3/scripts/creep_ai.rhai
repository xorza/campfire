fn think(ctx, unit) {
    let target = defend_hero(ctx, unit);
    if target == () {
        target = keep_target(unit);
    }
    if target == () {
        target = ctx.nearest_visible_enemy(unit, unit.params.aggro_range, "creep");
    }
    if target == () {
        target = ctx.nearest_visible_enemy(unit, unit.params.aggro_range, "hero");
    }
    if target == () {
        ctx.order_follow_lane(unit);
    } else if target != unit.target {
        ctx.order_attack(unit, target);
    }
}

fn defend_hero(ctx, unit) {
    for ally in ctx.allies_near(unit, unit.pos, unit.params.help_range) {
        if !ally.is_hero {
            continue;
        }
        for attacker in ally.recent_attackers(unit.params.help_window_ms) {
            if attacker.is_hero && attacker.is_enemy_of(unit) && unit.can_see(attacker) {
                return attacker;
            }
        }
    }
    ()
}

fn keep_target(unit) {
    let target = unit.target;
    if target != () && target.alive && unit.can_see(target) && unit.pos.distance_to(target.pos) <= unit.params.aggro_range {
        target
    } else {
        ()
    }
}
