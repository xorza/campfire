fn on_cast(ctx, caster, target) {
    ctx.projectile_homing(caster, caster, target, #{ speed: ctx.value("speed") });
}

fn on_projectile_hit(ctx, proj, target) {
    ctx.damage(proj.source, target, ctx.value("damage"), "magic");
    ctx.add_modifier(target, "dusk_mark");
}

// The mark itself: any other damage from Veil detonates it, including her attacks.
fn on_damage_taken(ctx, m, source, dmg) {
    if source != m.source || dmg.ability == "dusk_mark" {
        return;
    }
    ctx.damage(m.source, m.carrier, ctx.value("detonate_damage"), "magic");
    ctx.restore(m.source, "energy", ctx.value("energy_refund"));
    ctx.remove_modifier(m.carrier, "dusk_mark");
}
