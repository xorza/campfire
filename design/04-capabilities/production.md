# Production

Making units and buildings, as RTS games do. Also useful for base building in survival and MMO housing.

- **Build queues:** a building (or a unit) produces unit types from a queue, each with a cost in player resources, a time and requirements. Progress, cancel and refund are part of the capability; a rally point sends new units on.
- **Construction:** placing a building checks its footprint against grid occupancy and the mode's rules (power, creep, distance); the building then grows over time, or builders construct it.
- **Harvesting:** a gather order: a worker walks to a resource node, gathers over time, and returns the load to a drop-off. Nodes deplete.
- **Tech:** requirements are buildings, upgrades or levels a player owns; upgrades are modifiers on all units of a type.
- **Supply:** a cap on units per player, as a player resource.

Selection and control groups are client-side; orders name the units they go to.
