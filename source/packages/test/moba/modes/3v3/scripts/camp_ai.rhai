fn on_think(ctx, unit) {
    let away = unit.position.distance_to(unit.spawn_pos);
    if away > unit.params.leash_range {
        ctx.order_reset();
        return;
    }
    let target = unit.attack_target;
    if target != () && target.is_alive && unit.can_see(target) {
        return;
    }
    for attacker in unit.recent_attackers(unit.params.aggro_window_ms) {
        if unit.can_see(attacker) {
            ctx.order_attack(attacker);
            return;
        }
    }
    if away > unit.params.home_slack {
        ctx.order_move(unit.spawn_pos);
    }
}
