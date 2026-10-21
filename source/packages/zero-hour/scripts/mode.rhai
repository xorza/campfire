// A skirmish's start, as the original's `placeNetworkBuildingsForPlayer` places it: each player's
// faction building at its start waypoint, `Player_<n>_Start` of slot `n`, and each starting unit
// at the room nearest its rally waypoint, `Player_<n>_Rally`, or on a map with none the
// building's point moved its faction's `start_offset` along +z, on a grid of the original's
// pathfind cell, 10, as clear of each body, as the building may cover the rally; a unit with no
// room within 20 cells is left out. A player's faction is the one its host picked, or one
// drawn of the mode's factions, as the original draws a slot's Random, which then counts as its
// pick. A seat whose host picked the start `base` also gets its faction's base: each of its base
// buildings, whole, at the room nearest its start within 40 cells, and `base_count` of each of
// its base units at the room nearest its rally within 40, each left out with no room. A slot no
// player or bot held as the match started, and a player whose start the map lacks, start with
// no unit. Every player starts with the skirmish's money, `start_money`, as the game gives each
// its starting cash.
fn on_match_start(ctx) {
    let factions = ctx.offers("faction");
    for player in ctx.players {
        ctx.add_resource(player, "money", ctx.params.start_money);
        if !player.seated {
            continue;
        }
        let picked = ctx.chosen(player, "faction");
        let faction = if picked.is_empty() {
            let drawn = ctx.pick(factions);
            ctx.choose(player, "faction", drawn);
            drawn
        } else {
            picked[0]
        };
        let n = player.seat + 1;
        let starts = start_marker(ctx, `Player_${n}_Start`);
        if starts.is_empty() {
            continue;
        }
        let at = starts[0].position;
        let team = player.team;
        ctx.spawn_unit(ctx.params[`start_building_${faction}`], team, at, player);
        let rallies = start_marker(ctx, `Player_${n}_Rally`);
        let rally = if rallies.is_empty() {
            at + vector(0, 0, ctx.params[`start_offset_${faction}`])
        } else {
            rallies[0].position
        };
        for unit in ctx.params[`start_units_${faction}`] {
            spawn_near(ctx, player, unit, rally, 20);
        }
        let start = ctx.chosen(player, "start");
        if !start.is_empty() && start[0] == "base" {
            for building in ctx.params[`base_buildings_${faction}`] {
                spawn_near(ctx, player, building, at, 40);
            }
            for unit in ctx.params[`base_units_${faction}`] {
                for i in 0..ctx.params.base_count {
                    spawn_near(ctx, player, unit, rally, 40);
                }
            }
        }
    }
}

// A unit of `unit_type` of `player` at the room nearest `point` within `steps` pathfind cells, a
// cell clear of each body; none with no room.
fn spawn_near(ctx, player, unit_type, point, steps) {
    let at = ctx.room_near(unit_type, point, 10, steps);
    if at != () {
        ctx.spawn_unit(unit_type, player.team, at, player);
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
