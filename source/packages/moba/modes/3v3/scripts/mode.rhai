fn on_match_start(ctx) {
    ctx.timer("pick_end", ctx.value("pick_ms"), false, ());
}

fn on_mode_input(ctx, player, name, value) {
    if ctx.state.phase != "pick" {
        return;
    }
    if name == "hero" && ctx.hero_available(player, value) {
        ctx.choose_hero(player, value);
    } else if name == "spells" {
        ctx.choose_spells(player, value);
    }
}

fn on_timer(ctx, name, data) {
    if name == "pick_end" {
        ctx.state.phase = "play";
        ctx.spawn_heroes();
        for point in ctx.map.neutral_spawns {
            ctx.spawn_unit(point.unit_type, "neutral", point.pos);
        }
        ctx.timer("wave", ctx.value("first_wave_ms"), false, ());
        ctx.timer("income", ctx.value("income_ms"), true, ());
    } else if name == "wave" {
        for lane in ctx.map.lanes {
            for team in ctx.teams {
                let wave = ctx.value("wave");
                if inhibitor_down(ctx, ctx.enemy_team(team), lane) {
                    wave.push("super_creep");
                }
                ctx.spawn_wave(team, lane, wave);
            }
        }
        ctx.timer("wave", ctx.value("wave_ms"), false, ());
    } else if name == "income" {
        for hero in ctx.heroes() {
            ctx.add_resource(hero.owner, "gold", ctx.value("income_gold"));
        }
    }
}

fn on_unit_died(ctx, unit, killer) {
    if unit.has_tag("core") {
        ctx.end(ctx.enemy_team(unit.team));
        return;
    }
    share_xp(ctx, unit);
    if unit.is_hero {
        hero_died(ctx, unit, killer);
        return;
    }
    if killer != () && killer.is_hero {
        ctx.add_resource(killer.owner, "gold", unit.params.gold);
    }
    if unit.has_tag("inhibitor") {
        ctx.respawn(unit, ctx.value("inhibitor_respawn_ms"));
    } else if unit.has_tag("objective") {
        if killer != () {
            for hero in ctx.heroes_of(killer.team) {
                ctx.add_modifier(hero, "warden_blessing");
            }
        }
        ctx.respawn(unit, ctx.value("warden_respawn_ms"));
    } else if unit.has_tag("camp") {
        ctx.respawn(unit, ctx.value("camp_respawn_ms"));
    }
}

fn hero_died(ctx, hero, killer) {
    ctx.respawn(hero, ctx.value("respawn_base_ms") + ctx.value("respawn_per_level_ms") * hero.level);
    if killer == () || !killer.is_hero {
        return;
    }
    let gold = ctx.value("kill_gold");
    if !ctx.state.first_blood {
        gold += ctx.value("first_blood_gold");
        ctx.state.first_blood = true;
    }
    ctx.add_resource(killer.owner, "gold", gold);
    let assisters = [];
    for unit in hero.recent_attackers(ctx.value("assist_window_ms")) {
        if unit.is_hero && unit != killer {
            assisters.push(unit);
        }
    }
    for unit in assisters {
        ctx.add_resource(unit.owner, "gold", ctx.value("assist_gold") / assisters.len());
    }
}

fn share_xp(ctx, unit) {
    let heroes = ctx.enemy_heroes_near(unit, unit.pos, ctx.value("xp_radius"));
    if heroes.is_empty() {
        return;
    }
    let xp = if unit.is_hero {
        ctx.value("hero_xp_base") + ctx.value("hero_xp_per_level") * unit.level
    } else {
        unit.params.xp
    };
    for hero in heroes {
        ctx.add_xp(hero, xp / heroes.len());
    }
}

fn inhibitor_down(ctx, team, lane) {
    for unit in ctx.units_tagged("inhibitor") {
        if unit.team == team && unit.lane == lane {
            return !unit.alive;
        }
    }
    false
}

fn calc_damage(ctx, source, target, amount, kind) {
    if kind == "true" {
        return amount;
    }
    let physical = kind == "physical";
    let resist = if physical { target.stat("armor") } else { target.stat("magic_resist") };
    let pen_pct = if physical { source.stat("armor_pen_pct") } else { source.stat("magic_pen_pct") };
    let pen_flat = if physical { source.stat("armor_pen") } else { source.stat("magic_pen") };
    if resist > 0 {
        resist = (resist * (1 - pen_pct) - pen_flat).max(0);
    }
    let result = if resist >= 0 {
        amount * 100 / (100 + resist)
    } else {
        amount * (2 - num(100) / (100 - resist))
    };
    if physical {
        result = (result - target.stat("physical_block")).max(0);
    }
    result
}
