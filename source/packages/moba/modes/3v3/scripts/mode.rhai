fn on_match_start(ctx) {
    ctx.timer("pick_end", ctx.p.pick_ms, false, ());
}

fn on_mode_input(ctx, player, name, value) {
    if ctx.state.phase != "pick" {
        return;
    }
    if name == "hero" && ctx.available(player, "hero", value) {
        ctx.choose(player, "hero", value);
    } else if name == "spells" && value.len() == 2 {
        ctx.choose(player, "spells", value);
    }
}

fn on_timer(ctx, name, data) {
    if name == "pick_end" {
        ctx.state.phase = "play";
        spawn_heroes(ctx);
        for camp in ctx.map.markers("camp") {
            ctx.spawn_unit(camp.params.unit_type, "camps", camp.pos);
        }
        ctx.timer("wave", ctx.p.first_wave_ms, false, ());
        ctx.timer("income", ctx.p.income_ms, true, ());
    } else if name == "wave" {
        for lane in ctx.map.paths {
            for team in ctx.teams {
                let wave = ctx.p.wave;
                if inhibitor_down(ctx, ctx.enemy_team(team), lane) {
                    wave.push("super_creep");
                }
                ctx.spawn_group(team, lane, lane_end(ctx, team), wave);
            }
        }
        ctx.timer("wave", ctx.p.wave_ms, false, ());
    } else if name == "income" {
        for hero in ctx.avatars() {
            ctx.add_resource(hero.owner, "gold", ctx.p.income_gold);
        }
    }
}

fn on_unit_died(ctx, unit, killer, assisters) {
    if unit.has_tag("core") {
        ctx.end(ctx.enemy_team(unit.team));
        return;
    }
    if unit.has_tag("ward") {
        if killer != () && killer.is_avatar {
            ctx.add_resource(killer.owner, "gold", unit.params.gold);
        }
        return;
    }
    share_xp(ctx, unit);
    if unit.is_avatar {
        hero_died(ctx, unit, killer, assisters);
        return;
    }
    if killer != () && killer.is_avatar {
        ctx.add_resource(killer.owner, "gold", unit.params.gold);
    }
    if unit.has_tag("inhibitor") {
        ctx.respawn(unit, ctx.p.inhibitor_respawn_ms);
    } else if unit.has_tag("objective") {
        if killer != () {
            for hero in ctx.avatars(killer.team) {
                ctx.add_modifier(hero, "warden_blessing");
            }
        }
        ctx.respawn(unit, ctx.p.warden_respawn_ms);
    } else if unit.has_tag("camp") {
        ctx.respawn(unit, ctx.p.camp_respawn_ms);
    }
}

fn hero_died(ctx, hero, killer, assisters) {
    ctx.respawn(hero, ctx.p.respawn_base_ms + ctx.p.respawn_per_level_ms * hero.level);
    if killer == () || !killer.is_avatar {
        return;
    }
    let gold = ctx.p.kill_gold;
    if !ctx.state.first_blood {
        gold += ctx.p.first_blood_gold;
        ctx.state.first_blood = true;
    }
    ctx.add_resource(killer.owner, "gold", gold);
    for unit in assisters {
        ctx.add_resource(unit.owner, "gold", ctx.p.assist_gold / assisters.len());
    }
}

fn share_xp(ctx, unit) {
    let heroes = ctx.find(unit, unit.pos, ctx.p.xp_radius, "enemies:avatar");
    if heroes.is_empty() {
        return;
    }
    let xp = if unit.is_avatar {
        ctx.p.hero_xp_base + ctx.p.hero_xp_per_level * unit.level
    } else {
        unit.params.xp
    };
    for hero in heroes {
        ctx.add_xp(hero, "level", num(xp) / heroes.len());
    }
}

// A player who joins in the play phase with no hero gets the hero they chose, or the first one
// no player holds. A player who takes a bot's slot keeps the bot's hero.
fn on_player_join(ctx, player) {
    if ctx.state.phase != "play" || has_hero(ctx, player) {
        return;
    }
    if ctx.chosen(player, "hero").is_empty() {
        for hero in ctx.offers("hero") {
            if ctx.available(player, "hero", hero) {
                ctx.choose(player, "hero", hero);
                break;
            }
        }
    }
    spawn_hero(ctx, player);
}

fn spawn_heroes(ctx) {
    for player in 0..ctx.players {
        spawn_hero(ctx, player);
    }
}

// A player who chose a hero gets it at their team's spawn, with the spells they chose.
fn spawn_hero(ctx, player) {
    let hero = ctx.chosen(player, "hero");
    if hero.is_empty() {
        return;
    }
    let team = ctx.team_of(player);
    let unit = ctx.spawn_unit(hero[0], team, spawn_marker(ctx, team).pos, player);
    ctx.grant(unit, "spell", ctx.chosen(player, "spells"));
}

fn has_hero(ctx, player) {
    for hero in ctx.avatars() {
        if hero.owner == player {
            return true;
        }
    }
    false
}

// The end of the lanes `team`'s creeps walk from, as its spawn marker names it.
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

fn inhibitor_down(ctx, team, lane) {
    for unit in ctx.units_tagged("inhibitor") {
        if unit.team == team && unit.path == lane {
            return !unit.alive;
        }
    }
    false
}

// A source that is gone, as from a projectile that outlived it, has no bonus and no penetration.
fn source_stat(d, name) {
    let source = d.source;
    if source == () { 0 } else { source.stat(name) }
}

fn calc_damage(ctx, d) {
    if d.target.has_tag("ward") {
        return num(1);
    }
    let amount = d.amount * (1 + source_stat(d, "damage_dealt_pct"));
    if d.roll != () && d.roll < source_stat(d, "crit_chance") {
        amount *= 2;
    }
    if d.kind == "true" {
        return amount;
    }
    let physical = d.kind == "physical";
    let resist = if physical { d.target.stat("armor") } else { d.target.stat("magic_resist") };
    let pen_pct = source_stat(d, if physical { "armor_pen_pct" } else { "magic_pen_pct" });
    let pen_flat = source_stat(d, if physical { "armor_pen" } else { "magic_pen" });
    if resist > 0 {
        resist = (resist * (1 - pen_pct) - pen_flat).max(0);
    }
    if resist >= 0 {
        amount = amount * 100 / (100 + resist);
    } else {
        amount = amount * (2 - num(100) / (100 - resist));
    }
    if physical {
        amount = (amount - d.target.stat("physical_block")).max(0);
    }
    amount
}
