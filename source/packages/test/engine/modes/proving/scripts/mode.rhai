// Each slot's player plays its hero from the start, holds its barracks, and starts with gold for
// one guard, whether the player joined before the start or after it; gold comes every income_ms.
// At the end of tick 0 the mode saves, each hero learns its basic abilities and the first wave
// comes, then a wave every wave_ms. A unit that dies gives experience to the enemy heroes near
// it, and gold to its killer's player. A hero that dies respawns respawn_ms later. The team whose
// tower falls loses.
fn on_match_start(ctx) {
    let heroes = ["hero-lancer", "hero-sage"];
    for player in ctx.players {
        ctx.choose(player, "hero", heroes[player.player_slot]);
        let team = player.team;
        ctx.spawn_unit(heroes[player.player_slot], team, spawn_marker(ctx, team).position, player);
        for marker in ctx.map.markers("barracks") {
            if marker.team == team {
                ctx.spawn_unit("barracks", team, marker.position, player);
            }
        }
        ctx.add_resource(player, "gold", ctx.params.income);
    }
    ctx.after("first_wave", 1);
    ctx.every("income", ctx.params.income_ms);
}

fn on_player_join(ctx, player) {
    ctx.state.joined = player.player_slot;
}

fn on_player_leave(ctx, player) {
    ctx.state.left = player.player_slot;
}

fn on_timer(ctx, name, data) {
    if name == "income" {
        for player in ctx.players {
            ctx.add_resource(player, "gold", ctx.params.income);
        }
        return;
    }
    for lane in ctx.map.paths {
        for team in ["north", "south"] {
            ctx.spawn_group(team, lane, PathEnd::named(spawn_marker(ctx, team).params.from), ctx.params.wave);
        }
    }
    if name == "first_wave" {
        ctx.save();
        for hero in ctx.avatars() {
            for slot in 0..hero.params.basics {
                ctx.learn(hero, slot);
            }
        }
        ctx.every("wave", ctx.params.wave_ms);
    }
}

fn on_unit_death(ctx, unit, killer, assisters) {
    let heroes = ctx.find(unit, unit.position, ctx.params.xp_radius, "enemies:avatar");
    if !heroes.is_empty() && unit.params.xp > 0 {
        for hero in heroes {
            ctx.add_xp(hero, "level", num(unit.params.xp) / heroes.len());
        }
    }
    if killer != () && killer.owner != () {
        ctx.add_resource(killer.owner, "gold", ctx.params.kill_gold);
    }
    if unit.has_tag("tower") {
        ctx.win(ctx.enemies(unit.team)[0]);
    } else if unit.is_avatar {
        ctx.respawn(unit, ctx.params.respawn_ms);
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
