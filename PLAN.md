# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Animated looks

The slice of [Animated looks](docs/design/animated-looks.md): a player of a Zero Hour skirmish sees infantry stand, walk, fire and die with the original's animations, a building rise as it is built and show its damage, and an upgrade change what its units show. One batch, one commit; its checks are that document's.

### A6. The check and the fold (the Zero Hour check, documents)

- [ ] The Zero Hour check's skirmish play pins a Ranger's states as it walks, fires and dies, and counts the states converted and dropped by cause.
- [ ] `09-zero-hour.md`'s losses "Deaths" and "A site's look" lose what the slice converts and gain "Doors", and the slice's decisions move into `02-engine-core.md`, `09-zero-hour.md`, `04-capabilities/combat.md` and `04-capabilities/production.md`, and `animated-looks.md` is deleted.
- Checks: the slice's; it ends when a player sees its checks in the client.

## Foundations
