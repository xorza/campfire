# The console

What an agent's tools into a running match can take from the engine, and how MCP, Claude Code and other engines and games serve tools that drive a game, read on 26 October 2026: the engine from its code at `1e7f09814`; the others from their sources as linked.

## The engine

- **Bots are signed inputs.** `BotDriver` plays each bot's orders into its session through a `BotSink`, whose `send` logs the payload as the player slot's `Bot` input by `Session::serve_bot` (`session/host/src/server/bot_driver.rs`); the log records the player slot's controller as `Controller::Bot`.
- **A bot reads its team's view.** `BotView` reads the player's units, the units its team sees and the trains and builds each unit may start; `ServerBotView` sees by the match's vision, `ClientBotView` by what the client holds (`session/bots/src/bot_view.rs`).
- **A client plays an order script with no window.** `campfire-client --bot <orders file>` joins as a player with its own key and signs its orders in `send_orders` against `ClientBotView` (`network/net/src/sim_client/mod.rs`); the LAN check starts its bots so. A client runs no Rhai bot.
- **The pace pauses and sets a speed, and steps no tick.** `Pace` holds a pause and one of four speeds, which the client and its local server read each frame (`session/host/src/pace.rs`).
- **The journal's commands are a local server's.** `JournalCommand` saves at the boundary after the next tick, loads the latest save, or writes the log into a report; the client's keys push them into `PendingJournalCommands`, which its link carries to its local server (`session/host/src/messages/journal_command.rs`, `apps/client/src/local_keys.rs`).
- **No admin input exists.** The protocol's table of input sources lists an admin command among the external inputs (`05-protocol-spec.md`), and no code holds one.
- **The mode role changes a match.** The script API gives the mode `spawn_unit`, `spawn_group`, `respawn`, `grant` and `after`, and every role of the sim `add_modifier`, `restore` and `add_charge` (`06-script-api-reference.md`).
- **The screenshot is Bevy's, encoded by `codec`.** The report spawns `Screenshot::primary_window()` and encodes the captured `Image` through `codec::Png` (`apps/client/src/report.rs`).
- **Standard output is free.** `logging` writes text to standard error, and typed JSON lines into a file on request (`base/logging/src/lib.rs`).
- **`cargo drive` holds a step's streams.** Each step runs with `stdin(Stdio::null())`, its standard output and error piped through the driver's filter into its log (`drive` at `dcb1db7`, `src/capture.rs`).
- **The words.** `cargo drive vocabulary` counted 3,646 words in the names: `control` in 27 names and 314 uses in the code, `remote` in 39, `console` in 1, the private `on_console_event` of `platform/src/windows.rs`, `pilot` in 1, `operator` in 3, `tether` and `helm` in none.

## MCP

The revision of 2026-07-28 ([changelog](https://modelcontextprotocol.io/specification/latest/changelog)).

- **Stateless.** The revision removes the `initialize` handshake and the protocol's sessions; every request carries its protocol version and client capabilities in `_meta`, and a server that needs state across calls mints a handle that comes back as an ordinary tool argument.
- **Tasks are an extension.** Tasks move out of the core into `io.modelcontextprotocol/tasks`, polled by `tasks/get`.
- **Sampling is deprecated,** with the advice to "integrate directly with LLM provider APIs instead of Sampling".
- **A fixed order of tools.** Servers "SHOULD return tools from `tools/list` in a deterministic order", for clients' caches and prompt caches.
- **The stdio framing.** The stdio binding is "newline-delimited messages over the standard streams of a client-launched subprocess", and a custom transport over a byte stream "SHOULD reuse the stdio framing" ([transports](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports)).
- **A local server's access.** A server meant to run locally "SHOULD" use the `stdio` transport, or, over HTTP, require an authorization token or use Unix domain sockets ([security best practices](https://modelcontextprotocol.io/specification/2026-07-28/basic/security_best_practices)).
- **The stdio lifecycle.** A message must not hold a newline, and a server must write nothing else to its standard output; to stop a server, the client closes its input, waits, then terminates it, and servers "SHOULD exit promptly when their standard input is closed", "the only portable" graceful signal; a cancel is `notifications/cancelled`, after which the server "MUST NOT send any further messages" for the request ([stdio](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio)).
- **Two kinds of error.** An unknown tool or a malformed request is a JSON-RPC error, which models are "less likely" to fix; a failure of the call, a value out of range among them, is a result with `isError: true`, which clients "SHOULD" give the model so it can correct itself ([tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools)).
- **Structured results.** A result's `structuredContent` must match the tool's `outputSchema` when it declares one, and "SHOULD" come with its JSON in a text block too ([tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools)).
- **Handles.** A handle should be opaque, its lifetime stated in the description of the tool that makes it, and a call on an unknown one should fail as a result that says so ([tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools)).
- **The official Rust SDK.** `rmcp` serves stdio and Streamable HTTP, derives a tool's input schema through `schemars`, and supports the tasks extension ([rmcp](https://docs.rs/rmcp)).

## Claude Code

From its MCP documentation ([MCP](https://code.claude.com/docs/en/mcp)).

- **Limits of a call.** A tool call's limit defaults to about 28 hours; `MCP_TOOL_TIMEOUT` or a server's `timeout` sets another.
- **Limits of a result.** A warning past 10,000 tokens of output, a cut at 25,000, which `MAX_MCP_OUTPUT_TOKENS` changes; a text result past 50,000 characters goes to a file, unless the tool's `_meta["anthropic/maxResultSizeChars"]` raises its threshold, up to 500,000; a result with an image keeps the token limit.
- **Images.** A PNG, JPEG, GIF or WebP shows inline, scaled down or compressed to fit the model's limits, and its bytes are kept in a file.
- **Tasks.** In public beta, from v2.1.304, on revision 2026-07-28: a call moves to the background after two minutes. A user's report of 15 October 2026 says the extension does not yet work ([issue](https://claudeissues.com/issue/102347-feature-support-the-mcp-tasks-extension-io-modelcontextprotocol-tasks-spec-2026)).
- **Hints.** A tool of `readOnlyHint` changes nothing, and Claude Code runs its calls in parallel with other read-only calls; one of `idempotentHint` is safe to call twice, and Claude Code repeats it once over a new connection to an HTTP server.
- **Instructions.** With tool search, only the tools' names and the server's instructions load as a session starts; the instructions and an upfront tool's description are cut at 4,096 characters, a searched tool's at 16,384, and the search reads the shorter part.
- **Approval.** `_meta["anthropic/requiresUserInteraction"]` asks the user at every call of the tool, in every permission mode.
- **Structured results in practice.** Users' reports through 2026 say Claude Code drops a result's text or image blocks when `structuredContent` is also present, or shows the JSON in place of the text ([text dropped](https://claudeissues.com/issue/55677-mcp-tool-result-content-text-dropped-from-model-when-structuredcontent-is-also-p), [images dropped](https://claudeissues.com/issue/54737-bug-mcp-tool-results-image-content-blocks-dropped-when-structuredcontent-is-also), [JSON shown](https://claudeissues.com/issue/45575-tool-responses-with-both-content-text-and-structuredcontent-text-is-hidden)), and that only the first text block of an HTTP server's result reaches the model ([first block](https://claudeissues.com/issue/89630-mcp-http-tool-results-only-the-first-text-content-block-reaches-the-model-additi)).
- **A stdio server.** Claude Code sets `CLAUDE_PROJECT_DIR` in its environment, stops it after 30 minutes idle, and in an interactive session fetches the tool list again at `tools/list_changed`; a project's `.mcp.json` asks for approval once.

## Claude's images

From the vision guide ([Vision](https://platform.claude.com/docs/en/build-with-claude/vision)).

- **The cost.** An image costs `⌈width / 28⌉ × ⌈height / 28⌉` visual tokens.
- **The limits by model.** Claude 4.7 and later take a long edge of 2,576 px and 4,784 visual tokens, other models 1,568 px and 1,568; a larger image is scaled down, its aspect kept.
- **Many images.** A request of more than 20 images, those of earlier turns and of tool results among them, holds each image to a stricter limit, and rejects one past it; the guide advises no side past 2,000 px.
- **Formats.** JPEG, PNG, GIF and WebP; a lossy format's artifacts can hurt what the model reads.
- **Coordinates.** "Claude returns absolute pixel coordinates relative to the image it processes after resizing."

## Windows that draw no frame

- **A compositor stops a hidden window's frames.** Under Wayland, a window that is hidden or wholly covered stops receiving frame callbacks: Weston gives an occluded client none, Mutter blocks them as a window is occluded and marks it suspended 3 s later, and Qt's client filled its socket with requests no callback answered ([Qt Wayland](https://invent.kde.org/qt/qt/qtwayland/-/merge_requests/16), [Weston](https://gitlab.freedesktop.org/wayland/weston/-/merge_requests/1524), [Chromium](https://chromium.googlesource.com/chromium/src/+/69f60ea35d33ea693ae1ed6f65659943348101d7)).
- **`bevy_brp_mcp` says the same of its screenshots:** a minimized or hidden window may give a black image ([bevy_brp_mcp](https://docs.rs/bevy_brp_mcp/latest/bevy_brp_mcp/)).

## Tool design

- **Anthropic's guide.** A few tools for whole tasks rather than one for each endpoint, grouped by prefixes; names over low-level ids; a `response_format` of concise and detailed; pages, filters and truncation with sensible defaults; errors that say how to fix the call; and a prototype tested on real tasks of many steps ([Writing effective tools for agents](https://www.anthropic.com/engineering/writing-tools-for-agents)).
- **Playwright's wait.** Playwright MCP's `browser_wait_for` waits for a text to appear or go, or for a time; one version caps its wait at 10 s ([browser_wait_for](https://glama.ai/mcp/servers/@cloudflare/playwright-mcp/tools/browser_wait_for)).
- **Postcard has no schema in its bytes.** Postcard is "NOT" a self-describing format: both ends share one schema, and the compatibility of its revisions is outside its scope ([wire format](https://postcard.jamesmunns.com/wire-format)).

## Other engines and games

- **Bevy's `bevy_brp_mcp`** is a stdio MCP server that starts a Bevy app and talks to it over the Bevy Remote Protocol on port 15702: queries and writes of components by reflection, a screenshot of the window, a camera or the box round an entity, and the app's logs read from files ([bevy_brp_mcp](https://docs.rs/bevy_brp_mcp/latest/bevy_brp_mcp/)).
- **The Factorio Learning Environment** has agents act by programs in a REPL against a typed API of about 27 tools; vision "has thus far not improved LLM performance", and a headless server runs faster ([paper](https://arxiv.org/html/2503.09617v1)).
- **OpenRA's developer mode** resolves 16 orders, as `DevGiveCash`, `DevKill` and `DevVisibility`, which the order stream and so the replay carry; the lobby option `cheats` admits them, and `ResolveOrder` returns at once while they are off; a game of one human player always admits them ([DeveloperMode.cs](https://github.com/OpenRA/OpenRA/blob/bleed/OpenRA.Mods.Common/Traits/Player/DeveloperMode.cs)).
- **Factorio's console** runs Lua by `/c`, and a script command disables achievements for the save for good ([Console](https://wiki.factorio.com/Console)).
- **StarCraft II's API** connects an agent as a player; in single-step mode "the game simulation only advances once all players issue a Step request", in realtime mode it advances on its own; its interfaces, raw data "designed for scripted AI", feature layers, the rendered frame and the score, are chosen as the game is set up ([protocol](https://github.com/Blizzard/s2client-proto/blob/master/docs/protocol.md)).
- **Unreal strips its console from Shipping builds,** and "the Cheat Manager is not instanced in shipping builds" ([UCheatManager](https://docs.unrealengine.com/4.27/en-US/API/Runtime/Engine/GameFramework/UCheatManager/)).
- **Source gates a cheat by the server.** A command flagged `FCVAR_CHEAT` works only while the server's `sv_cheats` is on, a variable the server replicates to its clients ([ConVar](https://developer.valvesoftware.com/wiki/ConVar)).
