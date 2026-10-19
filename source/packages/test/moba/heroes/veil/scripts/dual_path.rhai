fn on_attack_hit(ctx, m, d) {
    ctx.damage(d.target, "magic", m.carrier.stat("attack_damage") * ctx.params.on_hit_share);
}
