fn on_attack_hit(ctx, m, d) {
    ctx.add_modifier(d.target, "withered");
}
