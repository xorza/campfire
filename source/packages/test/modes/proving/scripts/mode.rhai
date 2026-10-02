// Each player plays its hero from the start, holds its barracks, and starts with gold for one
// guard; gold comes every income_ms. At the end of tick 0 each hero learns its basic abilities and
// the first wave comes, then a wave every wave_ms. A unit that dies gives experience to the enemy heroes near it,
// and gold to its killer's player. A hero that dies respawns respawn_ms later. The team whose
// tower falls loses.
fn on_match_start(ctx) {
    let heroes = ["hero-lancer", "hero-sage"];
    for player in 0..ctx.players {
        ctx.choose(player, "hero", heroes[player]);
        let team = ctx.team_of(player);
        ctx.spawn_unit(heroes[player], team, spawn_marker(ctx, team).pos, player);
        for marker in ctx.map.markers("barracks") {
            if marker.team == team {
                ctx.spawn_unit("barracks", team, marker.pos, player);
            }
        }
        ctx.add_resource(player, "gold", ctx.p.income);
    }
    ctx.timer("first_wave", 1, false, ());
    ctx.timer("income", ctx.p.income_ms, true, ());
}

fn on_timer(ctx, name, data) {
    if name == "income" {
        for player in 0..ctx.players {
            ctx.add_resource(player, "gold", ctx.p.income);
        }
        return;
    }
    for lane in ctx.map.paths {
        for team in ["north", "south"] {
            ctx.spawn_group(team, lane, PathEnd::named(spawn_marker(ctx, team).params.from), ctx.p.wave);
        }
    }
    if name == "first_wave" {
        for hero in ctx.avatars() {
            for slot in 0..hero.params.basics {
                ctx.learn(hero, slot);
            }
        }
        ctx.timer("wave", ctx.p.wave_ms, true, ());
    }
}

fn on_unit_died(ctx, unit, killer, assisters) {
    let heroes = ctx.find(unit, unit.pos, ctx.p.xp_radius, "enemies:avatar");
    if !heroes.is_empty() && unit.params.xp > 0 {
        for hero in heroes {
            ctx.add_xp(hero, "level", num(unit.params.xp) / heroes.len());
        }
    }
    if killer != () && killer.owner != () {
        ctx.add_resource(killer.owner, "gold", ctx.p.kill_gold);
    }
    if unit.has_tag("tower") {
        ctx.end(ctx.enemy_team(unit.team));
    } else if unit.is_avatar {
        ctx.respawn(unit, ctx.p.respawn_ms);
    }
}

fn spawn_marker(ctx, team) {
    for marker in ctx.map.markers("spawn") {
        if marker.team == team {
            return marker;
        }
    }
    throw "a team with no spawn marker";
}
