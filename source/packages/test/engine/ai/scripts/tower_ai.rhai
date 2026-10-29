fn on_think(ctx, tower) {
    let range = tower.attack_range;
    let target = defend_hero(ctx, tower, range);
    if target == () {
        let current = tower.attack_target;
        if current != () && current.is_alive && tower.can_reach(current, range) {
            target = current;
        }
    }
    if target == () {
        target = ctx.nearest_visible(tower, range, "enemies:creep");
    }
    if target == () {
        target = ctx.nearest_visible(tower, range, "enemies:avatar");
    }
    if target != () && target != tower.attack_target {
        ctx.order_attack(target);
    }
}

fn defend_hero(ctx, tower, range) {
    for ally in ctx.find(tower, tower.position, range, "allies:avatar") {
        for attacker in ally.recent_attackers(tower.params.help_window_ms) {
            if attacker.is_avatar && attacker.is_enemy_of(tower) && tower.can_reach(attacker, range) {
                return attacker;
            }
        }
    }
    ()
}
