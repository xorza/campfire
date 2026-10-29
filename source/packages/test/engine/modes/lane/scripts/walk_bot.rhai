// The bot `walk`: each think, it walks its player's hero to (to, 2), its param `to`.
fn on_bot_think(ctx) {
    let hero = ctx.own_units(ctx.chosen(ctx.player, "hero")[0]);
    ctx.order(hero, #{ orders: #{ move: #{ to: [ctx.params.to, 2] } } });
}
