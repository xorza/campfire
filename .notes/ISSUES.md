# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

- **Control.** **Plan: EX9.** A bot never lets out the riders its containers come with, as China's Troop Crawlers with their 8 Red Guards, so those riders take no part in its fights. Zero Hour's crawler is an assault transport: an attack or attack-move order lets out each rider above half its life to fight and calls back each below it to heal; an idle order calls all back (`AssaultTransportAIUpdate`). Options: an assault transport capability, a container's option that its attack orders lets out and calls back its riders, Zero Hour's way, which every player's crawler gains; the bot evacuates a container as its army reaches the enemy; a base start spawns its containers empty; or the riders stay inside, a loss of Zero Hour beside "a bot that rides or garrisons".

## Research

- **Combat.** **Plan: EX3.** An Inferno Cannon was seen to fire at a target within its 50 m minimum range, which its weapon's action holds.
- **Interaction.** **Plan: EX1.** A use or an entry order of a unit that the use orders' query finds a use for, but that holds no slots, times or progress, panics the client's local server in `UseOrders::ordered` (`a user holds its slots, their times and their progress: QueryDoesNotMatch`), as in a China match against the America builder bot on Tournament Desert three minutes in.

## Ready

- **Interaction.** **Plan: EX8.** A container that leaves the match by a despawn, as a sale removes a building, leaves its riders out of the world with no container to let them out.
- **Navigation.** **Plan: EX10.** America's base start on Tournament Desert places its first Ranger where its buildings enclose it, so no order walks it out.
- **Zero Hour.** **Plan: EX2.** The 39 weapons of a set an upgrade chooses play no fire sound and no fire effect, as the effects import looks them up by the weapon's name, not by their converted actions' names.
