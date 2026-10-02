# Package

Design: [Modules](../../design/02-engine-core.md#modules), `package`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

## Ready

- **Plan: N2.** A script that makes a function pointer, a closure, an anonymous function or a call of `Fn`, loads, though design 08's checks refuse it; its `.call` fails the load as a member no handle has, not as a function pointer.
- **Stage 5.** Every name the [script API reference](../../design/08-script-api-reference.md) lists as planned loads and does nothing: a call or a field fails when a script uses it, and a hook never runs. Rime's Snow Owl fails on `ctx.reveal` as it ends.
