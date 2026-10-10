// The first team wins as the match starts, so a match shows the script ran.
fn on_match_start(ctx) {
    ctx.end(ctx.teams[0]);
}
