# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

## Ready

- **Airfields.** A jet parked at its airfield heals nothing, where Zero Hour heals it by its airfield's `HealAmountPerSecond`; a jet with no airfield and none of its team's to go to loses no life, where Zero Hour takes its `OutOfAmmoDamagePerSecond`; and a jet on the ground of an airfield that dies stays there, where Zero Hour kills it (`ParkingPlaceBehavior::killAllParkedUnits`).

- **Airfields.** A jet that goes home drops the order it held and takes none up after its reload, where Zero Hour resumes its last command (`JetAIUpdate`, `HAS_PENDING_COMMAND`).
