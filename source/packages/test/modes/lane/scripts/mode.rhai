// Player 0 plays the walker from the start, and player 1, when there is one, the runner. At the end
// of tick 0 each hero learns its first ability and the first wave comes, then a wave every wave_ms.
// A hero that dies respawns respawn_ms later.
fn on_match_start(ctx) {
    ctx.choose_avatar(0, "hero-walker");
    if ctx.players > 1 {
        ctx.choose_avatar(1, "hero-runner");
    }
    ctx.spawn_avatars();
    ctx.timer("first_wave", 1, false, ());
}

fn on_timer(ctx, name, data) {
    for lane in ctx.map.paths {
        for team in ctx.teams {
            ctx.spawn_group(team, lane, ctx.p.wave);
        }
    }
    if name == "first_wave" {
        for hero in ctx.avatars() {
            ctx.learn(hero, 0);
        }
        ctx.timer("wave", ctx.p.wave_ms, true, ());
    }
}

fn on_unit_died(ctx, unit, killer, assisters) {
    if unit.is_avatar {
        ctx.respawn(unit, ctx.p.respawn_ms);
    }
}
