# Navigation

Design: [Navigation](../design/04-capabilities/navigation.md). Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

## Ready

- **Plan: T1.** A map cannot block cells: design 04's navigation gives `[navigation]` the cells its map blocks on each layer, but the map data and `PathingGrid` hold none. The exact tests of a route, smoothing and the straight-goal shortcut, know only bodies.
