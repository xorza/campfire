# Plan — next steps

Open items only, in order. Remove an item when it is done. Each step ends with the check chain passing for the crates it touches, and a stop for review. Stage context: [ROADMAP.md](ROADMAP.md); design: [design/](design/).

1. **Hero death and respawn**: the Resolve stage calls the mode's `on_unit_died` with the killer and the assisters, and `ctx.respawn(unit, ms)` brings a dead unit back at its spawn after that time, with full health. The lane mode respawns each hero after a few seconds. Test: a hero that a tower kills stays dead on the server and its client for the respawn time, then stands at its team's spawn on both, with no rollback on the client.
