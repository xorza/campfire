# Quests

## Mechanism

Planned. What a player is asked to do, and what the world says to them: quests with stages and objectives, and dialogue with topics and choices. One mechanism for an RPG's quests, an MMO's quest givers and an RTS mission's objectives, as Skyrim's quests and Warcraft III's objectives track stages.

## Data

A package's `[quests.<id>]`: its `stages`, each with its objectives and the stage it leads to, and its `script`. An objective is a kind and a target: `reach` a marker, `kill` units a filter selects, `bring` items of a type, `talk` to a unit, or `custom`, which only the script completes; each may count (`kill` 5). A package's `[topics.<id>]`: who says it (a filter), a condition (a quest's stage, a track's level, a relation), its lines as message ids ([Human text](../03-game-scripting.md#game-package)), and its `choices`, each leading to a topic or an effect list.

## Rules

- **Stages.** `ctx.quest_stage(player, id, stage)` sets the stage of a player's quest; a stage's objectives complete from events: a region event for `reach`, a death for `kill`, an item gained for `bring`, a topic said for `talk`. When every objective completes, `on_stage_done(ctx, player, quest, stage)` runs and the quest goes to the next stage; a quest with none is done.
- **Dialogue.** A player who uses a unit with topics sees those whose conditions hold, by id; a choice is a command of `quests`, which runs its effect list or opens its topic. Lines go to the client only, in the player's language.
- **Campaign objectives** are a mission's quests: "destroy the base" is a `kill` objective, and the mission ends from the last `on_stage_done`.

## State and derived

- **State:** each player's quests, stages and objectives' counts.

## Script API

`ctx.quest_stage(player, id, stage)`, `ctx.quest(player, id)`; the hook `on_stage_done(ctx, player, quest, stage)`.

## Network

A player's quests and open topics go to that player only.

## Cost

Each event an objective listens for costs a test of the quests that listen; a player's topics a test of their conditions when the player uses a unit.

## Genres

Skyrim's and Diablo's quests; an MMO's quest givers; an RTS campaign's objectives.
