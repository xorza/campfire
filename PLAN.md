# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Roads

The slice of [Roads](docs/design/roads.md): a player sees a Zero Hour map's roads, sidewalks, tracks and markings over the terrain, as the game draws them. Its batches are in; its checks are that document's.

### End of the slice

- [ ] A player sees Tournament Desert's roads in the client as the game draws them ([question](PLAN_QUESTIONS.md#roads-end-of-the-slice)); then the slice's decisions fold into the documents of their systems, `09-zero-hour.md` and `02-engine-core.md`, the code's citations of `roads.md` with them, and `roads.md` goes.

## Effects

The slice of [Effects](docs/design/effects.md): a player of a Zero Hour skirmish sees each shot's fire effect at its barrel, each projectile's exhaust, each hit's burst and each death's explosion, as the game's FX lists and particle systems draw them. It starts once Roads ends. Its facts are [Zero Hour's effects](docs/research/zero-hour-effects.md); its checks are that document's.

### End of the slice

- [ ] A player sees Tournament Desert's shots, hits, trails and deaths in the client as the game draws them ([question](PLAN_QUESTIONS.md#effects-end-of-the-slice)); then the slice's decisions fold into the documents of their systems, `02-engine-core.md` for the format and the client's particles and `09-zero-hour.md` for what the import converts and loses, the code's citations of `effects.md` with them, and `effects.md` goes.

## Changed

- D6, a segment of no type is the release game's: `RoadDropCause::NoType` stays, for a segment of no type where the type of id 1 is a bridge or none, which the game draws nowhere; the install has none, and the Zero Hour check fails on one.

- E3, shots, hits and deaths play their effects: a projectile's hit and trail are named on the projectile type's own look, as its end and its trail, since the client knows a projectile's type and not the action that fired it; a shot's fire plays at its slot's first barrel bone of the first state of its look that names one, not of the state drawn at the shot; no match test plays the triggers, whose parts, the run's play of a list and the resolution of the looks' names, are tested.

## Foundations
