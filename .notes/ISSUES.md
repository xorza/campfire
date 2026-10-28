# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

- **Control.** A bot never lets out the riders its containers come with, as China's Troop Crawlers with their Red Guards, so those riders take no part in its fights, and the Zero Hour check's base match ends at tick 22,302, where it ended at 10,886 before the crawlers came full. Options: the bot evacuates a container of riders as its army reaches the enemy; a base start spawns its containers empty; or the bot keeps its riders inside, a loss of Zero Hour beside "a bot that rides or garrisons".

## Research

- **Interaction.** A use or an entry order of a unit that the use orders' query finds a use for, but that holds no slots, times or progress, panics the client's local server in `UseOrders::ordered` (`a user holds its slots, their times and their progress: QueryDoesNotMatch`), as in a China match against the America builder bot on Tournament Desert three minutes in.
- **Zero Hour.** America's base start on Tournament Desert places its first Ranger where its buildings enclose it, so no order walks it out.

## Ready

- **Interaction.** A container that leaves the match by a despawn, as a sale removes a building, leaves its riders out of the world with no container to let them out.
