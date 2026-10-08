# Navigation

Design: [Navigation](../design/04-capabilities/navigation.md). Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- `pathing_grid/one` moves by 8 % with where the linker places `Regions::label`: the same machine code at offset 32 of its 64-byte line takes 131.7 µs, and at offset 48 121.4 µs, as its instructions a cycle fall from 4.49 to 4.23. A change anywhere in the crate can move it.


## Ready

- `BodyBox::push_out` rounds the pushed centre to the nearest bit, so a body it pushes off a corner can end less than its radius from the box and still overlap it, where [Collision](../design/04-capabilities/navigation.md#collision) moves it until its edge touches the box's: in `a_body_is_pushed_out_of_a_box`, the body 0.5 m off each side of the corner (2, 1) ends 2·11 863 283² bits² from it, below 2⁴⁸, the square of its 1 m radius, so it stays a contact in each later tick.

