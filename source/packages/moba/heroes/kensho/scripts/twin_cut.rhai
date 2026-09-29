fn on_attack_hit(ctx, m, target, hit) {
    if hit.extra {
        return;
    }
    if m.stacks + 1 < ctx.value("every") {
        ctx.add_stacks(m, 1);
    } else {
        ctx.set_stacks(m, 0);
        ctx.attack_hit(m.carrier, target);
    }
}
