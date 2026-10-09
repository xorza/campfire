fn on_hit(ctx, caster, target, hit) {
    let damage = ctx.p.damage;
    if target.has_modifier("kindle") {
        damage *= ctx.p.ablaze_bonus;
    }
    ctx.damage(target, damage, "magic");
    ctx.add_modifier(target, "kindle");
}
