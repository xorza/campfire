# Navigation

Design: [Navigation](../design/04-capabilities/navigation.md). Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- `pathing_grid/one` moves by 8 % with where the linker places `Regions::label`: the same machine code at offset 32 of its 64-byte line takes 131.7 µs, and at offset 48 121.4 µs, as its instructions a cycle fall from 4.49 to 4.23. A change anywhere in the crate can move it.


## Ready

- `Collider::part` moves a pushed body by the overlap over the floor root of the squared distance between the centres, so the move overshoots by the share that root falls short: two centres 1 bit apart along each axis have the root 1 of 2, and part by √2 times their overlap.

