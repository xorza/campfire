fn on_think(ctx, unit) {
    let target = ctx.nearest_visible(unit, unit.params.aggro_range, "enemies");
    if target != () && target != unit.attack_target {
        ctx.order_attack(target);
    }
}
