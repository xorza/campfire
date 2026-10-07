# Control

Design: [Control](../design/04-capabilities/control.md). Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

## Ready

- A player's order may aim at a unit its vision group does not see: no code of `orders`, `actions` or `combat` reads `SeenBy`, so an attack or an action may name a hidden unit by its stable id, where the design ignores such an order ([Orders](../design/04-capabilities/control.md#orders)).
