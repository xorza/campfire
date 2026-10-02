fn on_think(ctx, unit) {
    let target = defend_hero(ctx, unit);
    if target == () {
        target = keep_target(unit);
    }
    if target == () {
        target = ctx.nearest_visible(unit, unit.params.aggro_range, "enemies:creep");
    }
    if target == () {
        target = ctx.nearest_visible(unit, unit.params.aggro_range, "enemies:structure");
    }
    if target == () {
        target = ctx.nearest_visible(unit, unit.params.aggro_range, "enemies:avatar");
    }
    if target == () {
        ctx.order_follow_path(unit);
    } else if target != unit.target {
        ctx.order_attack(unit, target);
    }
}

fn defend_hero(ctx, unit) {
    for ally in ctx.find(unit, unit.pos, unit.params.help_range, "allies:avatar") {
        for attacker in ally.recent_attackers(unit.params.help_window_ms) {
            if attacker.is_avatar && attacker.is_enemy_of(unit) && in_reach(unit, attacker) {
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

// Whether `unit` sees `other` within its aggro range, from the edge of the one body to the edge
// of the other, as `nearest_visible` counts it.
fn in_reach(unit, other) {
    let reach = unit.params.aggro_range + unit.radius + other.radius;
    unit.can_see(other) && unit.pos.within(other.pos, reach)
}
