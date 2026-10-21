fn on_match_start(ctx) {
    ctx.after("pick_end", ctx.params.pick_ms);
}

fn on_input(ctx, player, name, value) {
    if ctx.state.phase != "pick" {
        return;
    }
    if name == "hero" && ctx.can_choose(player, "hero", value) {
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
            ctx.spawn_unit(camp.params.unit_type, "camps", camp.position);
        }
        ctx.after("wave", ctx.params.first_wave_ms);
        ctx.every("income", ctx.params.income_ms);
    } else if name == "wave" {
        for lane in ctx.map.paths {
            for team in ctx.teams {
                let wave = ctx.params.wave;
                for enemy in ctx.enemies(team) {
                    if inhibitor_down(ctx, enemy, lane) {
                        wave.push("super_creep");
                        break;
                    }
                }
                ctx.spawn_group(team, lane, lane_end(ctx, team), wave);
            }
        }
        ctx.after("wave", ctx.params.wave_ms);
    } else if name == "income" {
        for hero in ctx.all_avatars() {
            ctx.add_resource(hero.owner, "gold", ctx.params.income_gold);
        }
    }
}

fn on_unit_death(ctx, unit, killer, assisters) {
    if unit.has_tag("core") {
        core_fell(ctx, unit);
        return;
    }
    if unit.has_tag("ward") {
        return;
    }
    if unit.is_avatar {
        hero_died(ctx, unit, killer, assisters);
        return;
    }
    if unit.has_tag("inhibitor") {
        ctx.respawn(unit, ctx.params.inhibitor_respawn_ms);
    } else if unit.has_tag("objective") {
        if killer != () {
            for hero in ctx.avatars(killer.team) {
                ctx.add_modifier(hero, "warden_blessing");
            }
        }
        ctx.respawn(unit, ctx.params.warden_respawn_ms);
    } else if unit.has_tag("camp") {
        ctx.respawn(unit, ctx.params.camp_respawn_ms);
    }
}

fn hero_died(ctx, hero, killer, assisters) {
    ctx.respawn(hero, ctx.params.respawn_base_ms + ctx.params.respawn_per_level_ms * hero.level);
    if killer == () || !killer.is_avatar {
        return;
    }
    let gold = ctx.params.kill_gold;
    if !ctx.state.first_blood {
        gold += ctx.params.first_blood_gold;
        ctx.state.first_blood = true;
    }
    ctx.add_resource(killer.owner, "gold", gold);
    // Only a hero's player takes gold: a tower or a creep that struck the hero takes no share.
    let heroes = [];
    for unit in assisters {
        if unit.is_avatar {
            heroes.push(unit);
        }
    }
    for assister in heroes {
        ctx.add_resource(assister.owner, "gold", ctx.params.assist_gold / heroes.len());
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
            if ctx.can_choose(player, "hero", hero) {
                ctx.choose(player, "hero", hero);
                break;
            }
        }
    }
    spawn_hero(ctx, player);
}

fn spawn_heroes(ctx) {
    for player in ctx.players {
        spawn_hero(ctx, player);
    }
}

// A player who chose a hero gets it at the room nearest their team's spawn, a meter clear of the
// heroes spawned with it, with the spells they chose.
fn spawn_hero(ctx, player) {
    let hero = ctx.chosen(player, "hero");
    if hero.is_empty() {
        return;
    }
    let team = player.team;
    let at = ctx.room_near(hero[0], spawn_marker(ctx, team).position, 1, 8);
    let unit = ctx.spawn_unit(hero[0], team, at, player);
    ctx.grant(unit, "spell", ctx.chosen(player, "spells"));
}

fn has_hero(ctx, player) {
    for hero in ctx.all_avatars() {
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
    for unit in ctx.all_units_tagged("inhibitor") {
        if unit.team == team && unit.path == lane {
            return !unit.is_alive;
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

// A core's fall ends the match, its enemy the winner while the enemy's core stands, else a draw,
// as when both fall in one tick. Each death of a tick is heard after all of them, so the first
// core heard ends the match, and `ctx.result` passes over the other.
fn core_fell(ctx, core) {
    if ctx.result != () {
        return;
    }
    let enemies = ctx.enemies(core.team);
    for other in ctx.units_tagged("core") {
        if other.is_alive && other.team in enemies {
            ctx.win(other.team);
            return;
        }
    }
    ctx.draw();
}
