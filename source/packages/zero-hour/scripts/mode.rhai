// A skirmish's start, as the original's `placeNetworkBuildingsForPlayer` places it: each player's
// faction building at its start waypoint, `Player_<n>_Start` of slot `n`, and each starting unit
// at its rally waypoint, `Player_<n>_Rally`, or on a map with none at the building's point moved
// `start_offset` along +z. A slot no player or bot held as the match started, and a player whose
// start the map lacks, start with no unit. Every player starts with the skirmish's money,
// `start_money`, as the game gives each its starting cash.
fn on_match_start(ctx) {
    for player in 0..ctx.players {
        ctx.add_resource(player, "money", ctx.p.start_money);
        if !ctx.held(player) {
            continue;
        }
        let n = player + 1;
        let starts = start_marker(ctx, `Player_${n}_Start`);
        if starts.is_empty() {
            continue;
        }
        let at = starts[0].pos;
        let team = ctx.team_of(player);
        ctx.spawn_unit(ctx.p.start_building, team, at, player);
        let rallies = start_marker(ctx, `Player_${n}_Rally`);
        let rally = if rallies.is_empty() {
            at + vector(0, 0, ctx.p.start_offset)
        } else {
            rallies[0].pos
        };
        for unit in ctx.p.start_units {
            ctx.spawn_unit(unit, team, rally, player);
        }
    }
}

// A skirmish's end, as the original's `VictoryConditions` keeps it: a player whose last living
// building that counts for victory died is defeated, out of play, and its other units die, as
// `Player::killPlayer` kills them; once the buildings that count left alive are all of one team,
// that team wins, and of none, the match is a draw. Each death of a tick is heard after all of
// them, so every one finds the same buildings left, and only the first ends the match, which the
// mode's state `ended` holds.
fn on_unit_died(ctx, unit, killer, assisters) {
    if !unit.has_tag("counts_for_victory") || unit.owner == () {
        return;
    }
    let player = unit.owner;
    let counting = ctx.units_tagged("counts_for_victory");
    for other in counting {
        if other.alive && other.owner == player {
            return;
        }
    }
    ctx.set_out(player, true);
    for other in ctx.units_of(player) {
        ctx.kill(other);
    }
    if ctx.state.ended {
        return;
    }
    let teams = [];
    for other in counting {
        if other.alive && other.owner != () && !(other.team in teams) {
            teams.push(other.team);
        }
    }
    if teams.is_empty() {
        ctx.state.ended = true;
        ctx.end(());
    } else if teams.len() == 1 {
        ctx.state.ended = true;
        ctx.end(teams[0]);
    }
}

// The start or rally waypoint of the name `name`, as a list of none or one.
fn start_marker(ctx, name) {
    for marker in ctx.map.markers("start") {
        if marker.params.name == name {
            return [marker];
        }
    }
    []
}
