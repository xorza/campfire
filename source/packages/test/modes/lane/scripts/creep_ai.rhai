fn think(ctx, unit) {
    let target = defend_hero(ctx, unit);
    if target == () {
        target = keep_target(unit);
    }
    if target == () {
        target = ctx.nearest_visible(unit, unit.params.aggro_range, "enemies:creep");
    }
    if target == () {
        target = ctx.nearest_visible(unit, unit.params.aggro_range, "enemies:hero");
    }
    if target == () {
        ctx.order_follow_lane(unit);
    } else if target != unit.target {
        ctx.order_attack(unit, target);
    }
}

fn defend_hero(ctx, unit) {
    for ally in ctx.find(unit, unit.pos, unit.params.help_range, "allies:hero") {
        for attacker in ally.recent_attackers(unit.params.help_window_ms) {
            if attacker.is_hero && attacker.is_enemy_of(unit) && in_reach(unit, attacker) {
                return attacker;
            }
        }
    }
    ()
}

fn keep_target(unit) {
    let target = unit.target;
    if target != () && target.alive && in_reach(unit, target) {
        target
    } else {
        ()
    }
}

fn in_reach(unit, other) {
    unit.can_see(other) && unit.pos.within(other.pos, unit.params.aggro_range)
}
