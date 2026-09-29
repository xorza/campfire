fn on_toggle(ctx, caster, on) {
    if on {
        ctx.add_modifier(caster, "chill_arrows");
    } else {
        ctx.remove_modifier(caster, "chill_arrows");
    }
}

fn on_attack_hit(ctx, m, target, hit) {
    ctx.slow(target, ctx.value("slow"), ctx.value("slow_ms"));
}
