# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Slice: Commanded units

The player moves a group of any size, and the checks play under the client's terms ([Commanded units](docs/design/commanded-units.md)). Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. Commanded units in the match (L)

The payload's commands, `players`, the orders of every capability, order scripts, bots and the session log: one change of the protocol, as an order's body loses its units.

- **A payload's command goes to a capability or to the players,** in the sim's command and `TickInputs`, which keeps each command's place (A select is the players' command).
- **Each player holds its commanded units,** which a select sets in Inputs before the capabilities, and `TickCommands` gives each order the units commanded at its place (A player's orders go to its commanded units; An order goes to the units commanded at its place; A select holds what its player controls).
- **An order's body holds its directive alone;** order scripts and the bots' driver send a select when the units differ (A client and a bot send a select when the units differ).
- **The commanded units are sent to their player alone** and predicted (The commanded units are their player's prediction).
- **Check:** the slice's checks of a group of any size, of an order's place and of a select; the session log's tag moves; the verifier replays the logs.

### 2. The client commands its selection (M)

The client and its prediction. Last: the user moves a hundred units in the client.

- **The client puts a select before an order when its selection differs** from the units its predicted match commands (A client and a bot send a select when the units differ).
- **Check:** a client's select and order predict the server's start; the user plays a base start and moves a hundred units, and the log shows no dropped order. The slice's decisions fold into control.md, 05-protocol-spec.md and 02-engine-core.md, and its document goes.

## Foundations
