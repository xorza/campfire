fn on_interval(ctx, m) {
    ctx.add_stacks(m, 1);
}

fn on_attack(ctx, m, target) {
    ctx.set_stacks(m, 0);
}
