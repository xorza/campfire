fn on_hit(ctx, caster, target, hit) {
    ctx.damage(target, "magic", ctx.params.damage);
    ctx.add_modifier(target, "dusk_mark");
}

// On the marked unit: any other damage from the Veil who marked it, attacks included, detonates the mark.
fn on_damage_taken(ctx, m, d) {
    if d.source == m.source && !d.is_action("dusk_mark") {
        ctx.damage(m.carrier, "magic", ctx.params.detonate_damage);
        ctx.restore(m.source, "energy", ctx.params.energy_refund);
        ctx.remove_modifier(m);
    }
}
