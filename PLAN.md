# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Slice: Commanded units

The player moves a group of any size, and the checks play under the client's terms ([Commanded units](docs/design/commanded-units.md)). Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. The client commands its selection (M)

The client and its prediction. Last: the user moves a hundred units in the client.

- **The client leaves the select out** when its inputs' acks say the server commands the order's units (A select goes with each order until its sender knows the server commands those units).
- **A sender splits a select past one payload** into a select and the ones that continue it, the client and the bots alike, where an order to more units than one payload holds is now dropped as too large (A select holds what its player controls).
- **Check:** a client's select and order predict the server's start; the user plays a base start and moves a hundred units, and the log shows no dropped order. The slice's decisions fold into control.md, 05-protocol-spec.md and 02-engine-core.md, and its document goes.

## Foundations
