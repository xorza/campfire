fn on_attack_hit(ctx, m, target, hit) {
    ctx.damage(m.carrier, target, hit.damage * ctx.value("on_hit_share"), "magic");
}

fn on_damage_dealt(ctx, m, target, dmg) {
    if dmg.ability != "" {
        ctx.heal(m.carrier, dmg.amount * ctx.value("spell_vamp"));
    }
}
