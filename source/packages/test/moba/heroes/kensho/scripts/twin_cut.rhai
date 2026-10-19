fn on_attack_hit(ctx, m, d) {
    if d.is_extra {
        return;
    }
    m.stacks += 1;
    if m.stacks >= ctx.params.every {
        m.stacks = 0;
        ctx.attack_hit(d.target);
    }
}
