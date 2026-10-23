// Player 0 plays the walker from the start, and player 1, when there is one, the runner. At the end
// of tick 0 the first wave comes, then a wave every wave_ms. A hero that dies respawns respawn_ms
// later; the hero that fells a creep or the other hero takes its experience, as the mode's
// `[experience]` gives it. The team whose tower falls loses.
fn on_match_start(ctx) {
    let heroes = ["hero-walker", "hero-runner"];
    for player in ctx.players {
        ctx.choose(player, "hero", heroes[player.player_slot]);
        let team = player.team;
        ctx.spawn_unit(heroes[player.player_slot], team, spawn_marker(ctx, team).position, player);
    }
    ctx.after("first_wave", 1);
}

fn on_input(ctx, player, name, value) {
    ctx.state.picked = value;
}

fn on_timer(ctx, name, data) {
    for lane in ctx.map.paths {
        for team in ctx.teams {
            ctx.spawn_group(team, lane, lane_end(ctx, team), ctx.params.wave);
        }
    }
    if name == "first_wave" {
        ctx.every("wave", ctx.params.wave_ms);
    }
}

fn on_unit_death(ctx, unit, killer, assisters) {
    if unit.has_tag("tower") {
        ctx.win(ctx.enemies(unit.team)[0]);
        return;
    }
    if unit.is_avatar {
        ctx.respawn(unit, ctx.params.respawn_ms);
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
