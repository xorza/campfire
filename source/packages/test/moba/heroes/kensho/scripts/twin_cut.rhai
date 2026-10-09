fn on_attack_hit(ctx, m, d) {
    if d.extra {
        return;
    }
    m.stacks += 1;
    if m.stacks >= ctx.p.every {
        m.stacks = 0;
        ctx.attack_hit(d.target);
    }
}
