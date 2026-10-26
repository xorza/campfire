# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Effects

The slice of [Effects](docs/design/effects.md): a player of a Zero Hour skirmish sees each shot's fire effect at its barrel, each projectile's exhaust, each hit's burst and each death's explosion, as the game's FX lists and particle systems draw them. Its facts are [Zero Hour's effects](docs/research/zero-hour-effects.md); its checks are that document's.

### End of the slice

- [ ] A player sees Tournament Desert's shots, hits, trails and deaths in the client as the game draws them ([question](PLAN_QUESTIONS.md#effects-end-of-the-slice)); then the slice's decisions fold into the documents of their systems, `02-engine-core.md` for the format and the client's particles and `09-zero-hour.md` for what the import converts and loses, the code's citations of `effects.md` with them, and `effects.md` goes.

## Changed


- E3, shots, hits and deaths play their effects: a projectile's hit and trail are named on the projectile type's own look, as its end and its trail, since the client knows a projectile's type and not the action that fired it; no match test plays the triggers, whose parts, the run's play of a list and the resolution of the looks' names, are tested.

## Declined

- M1, a missile falls as its fuel ends: fuel was not the cause of the report's early end, the launch was; with its barrel, a Tomahawk's missile hits targets 150, 250 and 350 m away after 43, 58 and 74 ticks of flight, inside its 120 ticks of fuel, so only a missile that circles runs out, as `04-capabilities/actions.md` (Fuel ends every steered flight) refused the fall for.

## Foundations
