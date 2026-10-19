fn on_hit(ctx, caster, target, hit) {
    let damage = ctx.params.damage;
    if target.has_modifier("kindle") {
        damage *= ctx.params.ablaze_bonus;
    }
    ctx.damage(target, "magic", damage);
    ctx.add_modifier(target, "kindle");
}
