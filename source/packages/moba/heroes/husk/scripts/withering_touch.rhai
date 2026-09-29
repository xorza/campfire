fn on_attack_hit(ctx, m, target, hit) {
    ctx.add_modifier(target, "withered");
}
