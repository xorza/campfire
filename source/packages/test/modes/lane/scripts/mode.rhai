// Player 0 plays the walker from the start, and player 1, when there is one, the runner. The first
// wave comes at the end of tick 0, then every wave_ms.
fn on_match_start(ctx) {
    ctx.choose_hero(0, "hero-walker");
    if ctx.players > 1 {
        ctx.choose_hero(1, "hero-runner");
    }
    ctx.spawn_heroes();
    ctx.timer("first_wave", 1, false, ());
}

fn on_timer(ctx, name, data) {
    for lane in ctx.map.lanes {
        for team in ctx.teams {
            ctx.spawn_wave(team, lane, ctx.p.wave);
        }
    }
    if name == "first_wave" {
        ctx.timer("wave", ctx.p.wave_ms, true, ());
    }
}
