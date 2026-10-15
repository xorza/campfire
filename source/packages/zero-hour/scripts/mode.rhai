// A skirmish's start, as the original's `placeNetworkBuildingsForPlayer` places it: each player's
// faction building at its start waypoint, `Player_<n>_Start` of slot `n`, and each starting unit
// at its rally waypoint, `Player_<n>_Rally`, or on a map with none at the building's point moved
// `start_offset` along +z. A player whose start the map lacks starts with nothing.
fn on_match_start(ctx) {
    for player in 0..ctx.players {
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

// The start or rally waypoint of the name `name`, as a list of none or one.
fn start_marker(ctx, name) {
    for marker in ctx.map.markers("start") {
        if marker.params.name == name {
            return [marker];
        }
    }
    []
}
