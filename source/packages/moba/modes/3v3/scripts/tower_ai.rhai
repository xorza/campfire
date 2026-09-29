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
        target = ctx.nearest_visible(tower, range, "enemies:creep");
    }
    if target == () {
        target = ctx.nearest_visible(tower, range, "enemies:hero");
    }
    if target != () && target != tower.target {
        ctx.order_attack(tower, target);
    }
}

fn defend_hero(ctx, tower, range) {
    for ally in ctx.find(tower, tower.pos, range, "allies:hero") {
        for attacker in ally.recent_attackers(tower.params.help_window_ms) {
            if attacker.is_hero && attacker.is_enemy_of(tower) && tower.pos.distance_to(attacker.pos) <= range {
                return attacker;
            }
        }
    }
    ()
}
