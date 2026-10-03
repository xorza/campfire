# Combat

Design: [Combat](../design/04-capabilities/combat.md). Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

- When a heal that a combat event's hook queues applies. [Events](../design/04-capabilities/combat.md#events) says a hook's heal applies at once, when the hook returns; [Damage and heals](../design/04-capabilities/combat.md#damage-and-heals) puts every heal of the tick in the pass's one queue, with those the events queue at its end, and so does the code, `DamagePass::apply_effect`, as it does a leech heal. Options: the heal applies at once, before the next damage of the pass, and the sentence of one queue names the exception; or it joins the end of the queue, and Events drops "a heal at once".

## Research

## Ready

