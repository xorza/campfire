# Quests

## Mechanism

What a player is asked to do, and what the world says to them: quests with stages and objectives, and dialogue with topics and choices. One mechanism for an RPG's quests, an MMO's quest givers and an RTS mission's objectives, as Skyrim's quests and Warcraft III's mission objectives each track stages and show them to the player.

## Data

A package's `[quests.<id>]`: its `stages`, each with its objectives and the stage it leads to, and its `script`. An objective is a kind with its target: `reach` a marker, `kill` units a filter selects, `bring` items of a type, `talk` to a unit, or `custom`, which only the script completes; each may count (`kill` 5). A package's `[topics.<id>]`: the unit it is said by (a filter), a condition (a quest's stage, a track's level, a relation), its lines for the client, as message ids ([Human text](../03-game-scripting.md#game-package)), and its `choices`, each leading to a topic or to an effect list.

## Rules

- **Stages.** A quest a player has is at one stage; `ctx.quest_stage(player, id, stage)` sets it, and the objectives of a stage complete from events: a region event for `reach`, a death for `kill`, an item gained for `bring`, a topic said for `talk`. When every objective of a stage completes, the quest runs the stage's `on_stage_done(ctx, player, quest, stage)` hook and goes to the stage it leads to. A quest with no next stage is done.
- **Dialogue.** A player who uses a unit with topics sees the topics whose conditions hold, in the order of their ids; a choice is a command of `quests`, and runs its effect list or opens its topic. A topic's lines go to the client only, which shows them in the player's language; what a choice does is data.
- **Campaign objectives** are a mission's quests: an RTS mission's "destroy the base" is a `kill` objective, and the mission ends from the quest's last `on_stage_done`.

## State and derived

- **State:** each player's quests, their stages and their objectives' counts.

## Script API

`ctx.quest_stage(player, id, stage)`, `ctx.quest(player, id)`; the hook `on_stage_done(ctx, player, quest, stage)`.

## Network

A player's quests and the topics open to them go to that player only, so no other player learns them.

## Cost

Each event an objective listens for costs a test of the quests that listen for it; a player's topics cost a test of their conditions when the player uses a unit.

## Genres

Skyrim's and Diablo's quests; an MMO's quest givers; an RTS campaign's mission objectives; a MOBA has none.
