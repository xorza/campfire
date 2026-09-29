fn on_attack_hit(ctx, m, d) {
    ctx.damage(d.target, m.carrier.stat("attack_damage") * ctx.p.on_hit_share, "magic");
}
