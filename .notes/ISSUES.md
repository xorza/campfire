# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

- **Client.** A dying unit that leaves the client's sight before its keep time ends plays its death's end as it leaves, as the client takes its despawn for the end of its death (`MatchEffects::play_deaths`).
- **Import.** A `JetSlowDeathBehavior`, a `HelicopterSlowDeathBehavior`, a `BattleBusSlowDeathBehavior` and a `NeutronMissileSlowDeathBehavior`, 88 modules of the install, are read as no death at all: their delay keeps no unit, their effects are not converted, and none is counted as a loss (`IniDeath::module`).

## Ready

- **Import.** **Plan: W1.** A death's hulk and thrown debris, a slow death's weapons, and a tracked vehicle's tread dust and track marks are not converted, for a slice of wrecks after Effects ([Zero Hour's deaths](../docs/research/zero-hour-deaths.md)).


