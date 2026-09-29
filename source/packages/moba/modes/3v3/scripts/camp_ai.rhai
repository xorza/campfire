fn think(ctx, unit) {
    if unit.pos.distance_to(unit.spawn_pos) > unit.params.leash_range {
        ctx.order_reset(unit);
        return;
    }
    let target = unit.target;
    if target != () && target.alive && unit.can_see(target) {
        return;
    }
    for attacker in unit.recent_attackers(unit.params.aggro_window_ms) {
        if unit.can_see(attacker) {
            ctx.order_attack(unit, attacker);
            return;
        }
    }
    if unit.pos.distance_to(unit.spawn_pos) > 0 {
        ctx.order_move(unit, unit.spawn_pos);
    }
}
