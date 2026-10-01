fn on_think(ctx, tower) {
    let range = tower.attack_range;
    let target = defend_hero(ctx, tower, range);
    if target == () {
        let current = tower.target;
        if current != () && current.alive && reaches(tower, current, range) {
            target = current;
        }
    }
    if target == () {
        target = ctx.nearest_visible(tower, range, "enemies:creep");
    }
    if target == () {
        target = ctx.nearest_visible(tower, range, "enemies:avatar");
    }
    if target != () && target != tower.target {
        ctx.order_attack(tower, target);
    }
}

fn defend_hero(ctx, tower, range) {
    for ally in ctx.find(tower, tower.pos, range, "allies:avatar") {
        for attacker in ally.recent_attackers(tower.params.help_window_ms) {
            if attacker.is_avatar && attacker.is_enemy_of(tower) && reaches(tower, attacker, range) {
                return attacker;
            }
        }
    }
    ()
}

// Whether `other` is within `range` of `tower`, from the edge of the one body to the edge of the
// other, as the tower's attack counts it.
fn reaches(tower, other, range) {
    tower.pos.within(other.pos, range + tower.radius + other.radius)
}
