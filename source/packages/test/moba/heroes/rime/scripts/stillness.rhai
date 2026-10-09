fn on_interval(ctx, m) {
    m.stacks += 1;
}

fn on_attack(ctx, m, target) {
    m.stacks = 0;
}
