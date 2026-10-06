// Player 0 plays the walker from the start, and player 1, when there is one, the runner. At the end
// of tick 0 the first wave comes, then a wave every wave_ms. A hero that fells a creep or the
// other hero takes its experience, and a hero that dies respawns respawn_ms later. The team whose
// tower falls loses.
fn on_match_start(ctx) {
    let heroes = ["hero-walker", "hero-runner"];
    for player in 0..ctx.players {
        ctx.choose(player, "hero", heroes[player]);
        let team = ctx.team_of(player);
        ctx.spawn_unit(heroes[player], team, spawn_marker(ctx, team).pos, player);
    }
    ctx.timer("first_wave", 1, false, ());
}

fn on_mode_input(ctx, player, name, value) {
    ctx.state.picked = value;
}

fn on_timer(ctx, name, data) {
    for lane in ctx.map.paths {
        for team in ctx.teams {
            ctx.spawn_group(team, lane, lane_end(ctx, team), ctx.p.wave);
        }
    }
    if name == "first_wave" {
        ctx.timer("wave", ctx.p.wave_ms, true, ());
    }
}

fn on_unit_died(ctx, unit, killer, assisters) {
    if unit.has_tag("tower") {
        ctx.end(ctx.enemy_team(unit.team));
        return;
    }
    if unit.is_avatar {
        ctx.respawn(unit, ctx.p.respawn_ms);
    }
    if killer != () && killer.is_avatar {
        let xp = if unit.is_avatar { ctx.p.hero_xp } else { ctx.p.creep_xp };
        ctx.add_xp(killer, "level", xp);
    }
}

// The end of the lane `team`'s creeps walk from, as its spawn marker names it.
fn lane_end(ctx, team) {
    PathEnd::named(spawn_marker(ctx, team).params.from)
}

fn spawn_marker(ctx, team) {
    for marker in ctx.map.markers("spawn") {
        if marker.team == team {
            return marker;
        }
    }
    throw "a team with no spawn marker";
}
