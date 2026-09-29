fn think(ctx, tower) {
    let range = tower.attack_range;
    let target = defend_hero(ctx, tower, range);
    if target == () {
        let current = tower.target;
        if current != () && current.alive && tower.pos.distance_to(current.pos) <= range {
            target = current;
        }
    }
    if target == () {
        target = ctx.nearest_visible_enemy(tower, range, "creep");
    }
    if target == () {
        target = ctx.nearest_visible_enemy(tower, range, "hero");
    }
    if target != () && target != tower.target {
        ctx.order_attack(tower, target);
    }
}

fn defend_hero(ctx, tower, range) {
    for ally in ctx.allies_near(tower, tower.pos, range) {
        if !ally.is_hero {
            continue;
        }
        for attacker in ally.recent_attackers(tower.params.help_window_ms) {
            if attacker.is_hero && attacker.is_enemy_of(tower) && tower.pos.distance_to(attacker.pos) <= range {
                return attacker;
            }
        }
    }
    ()
}
