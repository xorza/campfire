# Interaction

A unit uses an object near it. One mechanism for doors, the bomb, loot, capture points, NPCs, vehicles and garrisons.

- **Interactables** in data: range, who may use them (team, tag), and whether use is instant or held for a time (plant, defuse, capture, revive).
- **Held use** is a channel: moving, damage or a stun can interrupt it, as the object's data says; progress may be shared (a capture point).
- **Effects** are script hooks: `on_use`, `on_use_done`, `on_use_cancel`. Opening a door changes level geometry and vision blockers.
- **Enter and exit:** a unit enters a vehicle, a transport or a building, and rides or garrisons inside; it acts from inside when the host object allows.
- **NPC dialogue** is a script with choices sent to one player; trading uses `items` shops.
