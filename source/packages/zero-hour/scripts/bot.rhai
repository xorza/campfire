// Zero Hour's skirmish bot, which the imported mode names twice: `builder`, of `army` 0, and
// `attacker`. Each think, a bot with no site under way orders one of its faction's builders to
// build the first building of its faction's base list it lacks and may start, at the nearest
// place round its start where the build passes, so the builder idles once its base is whole and
// builds a building again once one falls. Each bot sends its idle gatherers to the nearest node,
// and trains a gatherer while it holds fewer than `gatherers` for each drop-off. The attacker also
// trains its faction's army at each producer, the type it holds fewest of first; while it holds
// fewer than `army` of them and more are coming, its idle units go at an enemy seen within
// `guard` of its start, and once it holds `army`, or no more are coming, at the nearest enemy it
// sees within `sight`, or at the nearest enemy start: by attack-move, and within `reach` by an
// attack (09-zero-hour.md, A builder and an attacker; 04-capabilities/control.md, A bot gathers;
// 04-capabilities/control.md, An army attack-moves).
fn on_bot_think(ctx) {
    let me = ctx.player;
    let chosen = ctx.chosen(me, "faction");
    let home = start_point(ctx, me.player_slot);
    if chosen.is_empty() || home == () {
        return;
    }
    let faction = chosen[0];
    build(ctx, faction, home);
    gather(ctx, faction);
    if ctx.params.army > 0 {
        let coming = train(ctx, faction);
        fight(ctx, me, faction, home, coming);
    }
}

// Sends each idle gatherer to gather at the nearest node, and orders one more gatherer while the
// bot holds fewer than `gatherers` for each of its drop-offs.
fn gather(ctx, faction) {
    let kinds = ctx.params[`gatherers_${faction}`];
    let held = 0;
    for kind in kinds {
        for unit in ctx.own(kind) {
            held += 1;
            if ctx.idle(unit) {
                let slot = #{ slot: `gather_${kind}`, target: #{ unit: "all:node" } };
                ctx.order([unit], #{ orders: #{ slot: slot } });
            }
        }
    }
    let drop_offs = 0;
    for building in ctx.params[`drop_offs_${faction}`] {
        drop_offs += ctx.own(building).len();
    }
    if held >= drop_offs * ctx.params.gatherers {
        return;
    }
    let producers = ctx.own(ctx.params[`start_building_${faction}`]);
    for building in ctx.params[`base_buildings_${faction}`] {
        producers += ctx.own(building);
    }
    for producer in producers {
        for kind in kinds {
            if ctx.may_start(producer, `train_${kind}`) {
                let slot = #{ slot: `train_${kind}`, target: "none" };
                ctx.order([producer], #{ orders: #{ slot: slot } });
                return;
            }
        }
    }
}

// Orders a builder to build the first base building the bot lacks and may start, unless a site
// of its base is under way, which its builder builds on.
fn build(ctx, faction, home) {
    let buildings = ctx.params[`base_buildings_${faction}`];
    for building in buildings {
        for unit in ctx.own(building) {
            if unit.site_progress != () {
                return;
            }
        }
    }
    let builders = [];
    for builder in ctx.params[`start_units_${faction}`] {
        builders += ctx.own(builder);
    }
    let near = #{ near: [home.x, home.z], gap: ctx.params.place_gap, steps: ctx.params.place_steps };
    for building in buildings {
        if !ctx.own(building).is_empty() {
            continue;
        }
        let action = `build_${building}`;
        for builder in builders {
            if ctx.may_start(builder, action) {
                let target = #{ point: #{ at: near, angle: 0 } };
                ctx.order([builder], #{ orders: #{ build: #{ slot: action, target: target } } });
                return;
            }
        }
    }
}

// Orders each producer to train the army type it may start that the bot holds fewest of; whether
// more of the army is coming: a producer trains or may start a train.
fn train(ctx, faction) {
    let coming = false;
    let army = ctx.params[`army_units_${faction}`];
    let producers = ctx.own(ctx.params[`start_building_${faction}`]);
    for building in ctx.params[`base_buildings_${faction}`] {
        producers += ctx.own(building);
    }
    for producer in producers {
        let pick = ();
        let fewest = 0;
        for unit in army {
            if ctx.may_start(producer, `train_${unit}`) {
                let held = ctx.own(unit).len();
                if pick == () || held < fewest {
                    pick = unit;
                    fewest = held;
                }
            }
        }
        if pick != () {
            let slot = #{ slot: `train_${pick}`, target: "none" };
            ctx.order([producer], #{ orders: #{ slot: slot } });
        }
        coming = coming || pick != () || !producer.queue.is_empty();
    }
    coming
}

// Sends the army's idle units at a target: while it gathers, an enemy seen within `guard` of its
// start; once it holds `army`, or as many as it gets, as no more is `coming`, the nearest enemy
// its first unit sees within `sight`, or the nearest enemy start. Those within `reach` of the
// target attack the nearest enemy, as the scan passes over a structure, and the others
// attack-move there. A unit that walks or fights goes on as it is.
fn fight(ctx, me, faction, home, coming) {
    let army = [];
    for unit in ctx.params[`army_units_${faction}`] {
        army += ctx.own(unit);
    }
    if army.is_empty() {
        return;
    }
    let idle = [];
    for unit in army {
        if ctx.idle(unit) {
            idle.push(unit);
        }
    }
    if idle.is_empty() {
        return;
    }
    let target = if army.len() < ctx.params.army && coming {
        let seen = ctx.find_visible(army[0], home, ctx.params.guard, "hostiles");
        if seen.is_empty() { () } else { seen[0].position }
    } else {
        let seen = ctx.nearest_visible(army[0], ctx.params.sight, "hostiles");
        if seen == () { nearest_enemy_start(ctx, me, home) } else { seen.position }
    };
    if target == () {
        return;
    }
    let close = [];
    let far = [];
    for unit in idle {
        if unit.position.within(target, ctx.params.reach) {
            close.push(unit);
        } else {
            far.push(unit);
        }
    }
    if !close.is_empty() {
        ctx.order(close, #{ orders: #{ attack: #{ target: "hostiles" } } });
    }
    if !far.is_empty() {
        ctx.order(far, #{ orders: #{ attack_move: #{ to: [target.x, target.z] } } });
    }
}

// The start of the enemy player placed nearest `from`; `()` with none.
fn nearest_enemy_start(ctx, me, from) {
    let enemies = ctx.enemies(me.team);
    let nearest = ();
    for player in ctx.players {
        if !player.held || !enemies.contains(player.team) {
            continue;
        }
        let start = start_point(ctx, player.player_slot);
        if start != () && (nearest == () || from.distance_to(start) < from.distance_to(nearest)) {
            nearest = start;
        }
    }
    nearest
}

// The point of the start waypoint of player slot `player_slot`, `Player_<n>_Start` of slot `n`; `()` on a map
// with none.
fn start_point(ctx, player_slot) {
    let name = `Player_${player_slot + 1}_Start`;
    for marker in ctx.map.markers("start") {
        if marker.params.name == name {
            return marker.position;
        }
    }
    ()
}
