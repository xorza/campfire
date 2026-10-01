fn on_hit(ctx, caster, target, hit) {
    ctx.damage(target, ctx.p.damage, "magic");
    ctx.add_modifier(target, "dusk_mark");
}

// On the marked unit: any other damage from the Veil who marked it, attacks included, detonates the mark.
fn on_damage_taken(ctx, m, d) {
    if d.source == m.source && d.ability != "dusk_mark" {
        ctx.damage(m.carrier, ctx.p.detonate_damage, "magic");
        ctx.restore(m.source, "energy", ctx.p.energy_refund);
        ctx.remove(m);
    }
}
