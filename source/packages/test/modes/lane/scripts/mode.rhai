// Player 0 plays the walker from the start, and player 1, when there is one, the runner. At the end
// of tick 0 each hero learns its first ability and the first wave comes, then a wave every wave_ms.
// A hero that dies respawns respawn_ms later. The team whose tower falls loses.
fn on_match_start(ctx) {
    ctx.choose_avatar(0, "hero-walker");
    if ctx.players > 1 {
        ctx.choose_avatar(1, "hero-runner");
    }
    ctx.spawn_avatars("spawn");
    ctx.timer("first_wave", 1, false, ());
}

fn on_timer(ctx, name, data) {
    for lane in ctx.map.paths {
        for team in ctx.teams {
            ctx.spawn_group(team, lane, lane_end(ctx, team), ctx.p.wave);
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
    if unit.has_tag("tower") {
        ctx.end(ctx.enemy_team(unit.team));
    } else if unit.is_avatar {
        ctx.respawn(unit, ctx.p.respawn_ms);
    }
}

// The end of the lane `team`'s creeps walk from, as its spawn marker names it.
fn lane_end(ctx, team) {
    for marker in ctx.map.markers("spawn") {
        if marker.team == team {
            return marker.params.from;
        }
    }
    throw "a team with no spawn marker";
}
