fn on_think(ctx, unit) {
    let target = ctx.nearest_visible(unit, unit.params.aggro_range, "enemies");
    if target != () && target != unit.target {
        ctx.order_attack(unit, target);
    }
}
