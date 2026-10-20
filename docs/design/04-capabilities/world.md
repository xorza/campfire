# World

- **A large world is one session.** Planned: regions that sleep when no player is near, and assets that stream by region; an MMO's world and a singleplayer open world are the same mechanism.
- **A waking region catches up by script.** `on_wake` places its units where the time slept would have taken them, as Skyrim places a town's people by their schedules.
- **A world's sim is one thread,** as every sim is, and a world past its budget is measured before any plan for more cores. Refused: systems in parallel over regions, as Factorio found a desync that hung on the count of cores hidden from 2017 to 2024 ([FFF #415](https://direct.factorio.com/blog/post/fff-415)).
- **Scripts never write to a database.** Characters, inventories and the world's state last through checkpoints; a login is a carry load, and admin commands, purchases and calendar events are recorded inputs.
- **Game state in the sim, services outside.** Trade, parties, guilds, quests and faction relations are sim state; chat, mail and friend lists are server services.
