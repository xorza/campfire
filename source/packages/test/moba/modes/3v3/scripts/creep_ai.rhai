fn on_think(ctx, unit) {
    let target = defend_hero(ctx, unit);
    if target == () {
        target = keep_target(unit);
    }
    if target == () {
        target = ctx.nearest_visible(unit, unit.params.aggro_range, "enemies:creep");
    }
    // A mode whose creeps' `avatars_first` is 1 has them try an enemy avatar before a
    // structure; each other tries a structure first.
    if target == () && unit.params.avatars_first == 1 {
        target = ctx.nearest_visible(unit, unit.params.aggro_range, "enemies:avatar");
    }
    if target == () {
        target = ctx.nearest_visible(unit, unit.params.aggro_range, "enemies:structure");
    }
    if target == () {
        target = ctx.nearest_visible(unit, unit.params.aggro_range, "enemies:avatar");
    }
    if target == () {
        ctx.order_follow_path();
    } else if target != unit.attack_target {
        ctx.order_attack(target);
    }
}

fn defend_hero(ctx, unit) {
    for ally in ctx.find(unit, unit.position, unit.params.help_range, "allies:avatar") {
        for attacker in ally.recent_attackers(unit.params.help_window_ms) {
            if attacker.is_avatar && attacker.is_enemy_of(unit) && in_reach(unit, attacker) {
                return attacker;
            }
        }
    }
    ()
}

fn keep_target(unit) {
    let target = unit.attack_target;
    if target != () && target.is_alive && in_reach(unit, target) {
        target
    } else {
        ()
    }
}

// Whether `unit` sees `other` within its aggro range, as `nearest_visible` measures it.
fn in_reach(unit, other) {
    unit.can_see(other) && unit.can_reach(other, unit.params.aggro_range)
}
