use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::future;
use std::sync::Arc;

use campfire_codec::Json;
use campfire_common::ErrorReport;
use campfire_console::{
    CameraAsk, CameraPose, ConsoleAnswer, ConsoleAsk, ConsoleEvent, Detail, EventLevel, FieldValue,
    GameRole, NearestNames, PaceAsk, PaceStatus, Reply, RunAsk, RunStop, RunTo, SessionStatus,
    ShotAsk, ShotRegion, UnitSelection, UnitTable, UnitsAsk,
};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    JsonObject, ListToolsResult, MetaObject, PaginatedRequestParams, ServerCapabilities,
    ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::error::GameFailure;
use crate::game_kind::GameKind;
use crate::game_process::EventFilter;
use crate::games::Games;
use crate::tool_name::ToolName;
use crate::tool_params::{
    CameraParams, EventsParams, GameParams, PaceParams, RunParams, ShotParams, StartParams,
    UnitsParams,
};

/// The tools that start, read and drive this workspace's games, over MCP (02-engine-core.md, Tools
/// by outcome, by what they act on): `game_` acts on the processes, `sim_` on the match.
#[derive(Debug, Clone)]
pub(crate) struct GameTools {
    games: Arc<Games>,
}

/// What a call gives the agent: its text, and a screenshot's PNG, in base64, when it shows one
/// (02-engine-core.md, A result is one text block, and an image when it shows one).
#[derive(Debug)]
struct CallOutput {
    text: String,
    png: Option<String>,
}

impl CallOutput {
    const fn text(text: String) -> Self {
        Self { text, png: None }
    }
}

/// What a tool does, and so the hints it gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolEffect {
    /// It changes nothing.
    Reads,
    /// A repeat leaves what it changed as it was.
    Sets,
    /// It changes something a repeat changes again.
    Changes,
}

/// The server's instructions, which Claude Code loads as a session starts (02-engine-core.md, The
/// instructions say the loop).
const INSTRUCTIONS: &str = "Campfire's games, run and read by an agent. The loop: start a game \
with game_start, a local match of the client (`--local --data <dir> <mode dir>`), which starts \
paused after its first tick; run it with sim_run to a tick, a count of ticks or until a unit's \
state, an event, a script failure or the match's end, which pauses it there; read it with \
sim_status and sim_units, whose replies name the tick they read, and game_events for its log; \
then stop it with game_stop. A game is named by the handle game_start gives, or by none when it \
is the only one. A failed call says what to change.";

/// The most characters a tool's text result keeps inline in Claude Code.
const RESULT_CHARS: u32 = 200_000;

/// The rows of a table unless a call names a limit.
const TABLE_ROWS: u32 = 100;

/// The events of a read unless a call names a limit.
const EVENTS: u32 = 50;

/// The wall time of a run unless a call names one: under Claude Code's two minutes, after which
/// it sends a call to the background.
const RUN_WALL_SECONDS: u32 = 110;

/// The tools, in the one order `tools/list` gives.
fn tools() -> Vec<Tool> {
    ToolName::ALL.into_iter().map(tool_of).collect()
}

/// The tool `name`: its title, description, schema and effect.
fn tool_of(name: ToolName) -> Tool {
    match name {
        ToolName::GameStart => tool::<StartParams>(
            name,
            "Start a game",
            "Start a game: build the client and the server, then start one with its console; \
             returns once it answers, before its load, with its handle.",
            ToolEffect::Changes,
        ),
        ToolName::GameStop => tool::<GameParams>(
            name,
            "Stop a game",
            "Stop a game: close its console's input, wait, then end it; gives its exit status.",
            ToolEffect::Sets,
        ),
        ToolName::GameEvents => tool::<EventsParams>(
            name,
            "Read a game's log",
            "Read the latest events of a game's log, by level, target and tick.",
            ToolEffect::Reads,
        ),
        ToolName::SimStatus => tool::<GameParams>(
            name,
            "Read the match's status",
            "Read the match's status: its phase, tick, rate, pace, state hash, players' resources \
             and script failures.",
            ToolEffect::Reads,
        ),
        ToolName::SimUnits => tool::<UnitsParams>(
            name,
            "Table the match's units",
            "Table the match's units: of a player slot, a team, a type, ids or a probe state, \
             concise unless asked, in pages.",
            ToolEffect::Reads,
        ),
        ToolName::SimRun => tool::<RunParams>(
            name,
            "Run the match",
            "Run a local match at four times its rate to a tick, a count of ticks or until a \
             condition holds, then pause; gives the tick it paused at and why.",
            ToolEffect::Sets,
        ),
        ToolName::SimPace => tool::<PaceParams>(
            name,
            "Set the match's pace",
            "Pause a local match, play it on, or set its speed.",
            ToolEffect::Sets,
        ),
        ToolName::ViewScreenshot => tool::<ShotParams>(
            name,
            "Show the client's window",
            "Screenshot the client's window, a rectangle of the last screenshot's pixels or the \
             box round units, scaled to fit an image Claude reads at its own pixels.",
            ToolEffect::Reads,
        ),
        ToolName::ViewCamera => tool::<CameraParams>(
            name,
            "Move the client's camera",
            "Read the client's camera, set its pose, or turn it to a unit or a point.",
            ToolEffect::Sets,
        ),
    }
}

/// A tool of the arguments `P`.
fn tool<P: JsonSchema + 'static>(
    name: ToolName,
    title: &'static str,
    description: &'static str,
    effect: ToolEffect,
) -> Tool {
    let annotations = ToolAnnotations::with_title(title)
        .read_only(effect == ToolEffect::Reads)
        .destructive(false)
        .idempotent(effect != ToolEffect::Changes)
        .open_world(false);
    let meta = BTreeMap::from([("anthropic/maxResultSizeChars", RESULT_CHARS)]);
    let meta: JsonObject = Json::convert(&meta).expect("a map of numbers is a JSON object");
    Tool::new(name.name(), description, Arc::new(JsonObject::new()))
        .with_input_schema::<P>()
        .with_title(title)
        .with_annotations(annotations)
        .with_meta(MetaObject(meta))
}

impl GameTools {
    pub(crate) const fn new(games: Arc<Games>) -> Self {
        Self { games }
    }

    /// The text of the call `name` of `arguments`, cancelled by `cancelled`; a failure the call's
    /// result holds.
    async fn call(
        &self,
        name: ToolName,
        arguments: JsonObject,
        cancelled: impl Future<Output = ()>,
    ) -> Result<CallOutput, GameFailure> {
        let text = match name {
            ToolName::GameStart => {
                let params: StartParams = Self::read(&arguments)?;
                let args: Vec<OsString> = params.args.into_iter().map(OsString::from).collect();
                let profile = params.profile.unwrap_or_default();
                let launched = self.games.start(params.game, &args, profile).await?;
                let role = match launched.opening.role {
                    GameRole::Client => "client",
                    GameRole::Server => "server",
                };
                let mut text = format!(
                    "started game {} of the {role}, console {}",
                    launched.handle, launched.opening.fingerprint
                );
                if params.game == GameKind::Client {
                    text.push_str("; a local match starts paused after its first tick");
                }
                if launched.own_newer {
                    text.push_str(
                        "\nthis MCP server's binary on disk is newer than its process: restart \
                         it (/mcp) to load it",
                    );
                }
                text
            }
            ToolName::GameStop => {
                let params: GameParams = Self::read(&arguments)?;
                let stopped = self.games.stop(params.handle).await?;
                format!("stopped game {}: {}", stopped.handle, stopped.status)
            }
            ToolName::GameEvents => {
                let params: EventsParams = Self::read(&arguments)?;
                let game = self.games.get(params.handle).await?;
                let filter = EventFilter {
                    level: params.level.unwrap_or(EventLevel::Info),
                    target: params.target,
                    since_tick: params.since_tick,
                    limit: usize::try_from(params.limit.unwrap_or(EVENTS)).expect("a u32 fits"),
                };
                Self::events_text(&game.events(&filter))
            }
            ToolName::SimStatus => {
                let params: GameParams = Self::read(&arguments)?;
                let game = self.games.get(params.handle).await?;
                Self::reply_text(&game.ask(ConsoleAsk::Status).await?)
            }
            ToolName::SimUnits => {
                let params: UnitsParams = Self::read(&arguments)?;
                let game = self.games.get(params.handle).await?;
                let ask = UnitsAsk {
                    units: params.units.unwrap_or(UnitSelection::All),
                    detail: params.detail.unwrap_or(Detail::Concise),
                    first: params.first.unwrap_or(0),
                    limit: Some(params.limit.unwrap_or(TABLE_ROWS)),
                };
                Self::reply_text(&game.ask(ConsoleAsk::Units(ask)).await?)
            }
            ToolName::SimRun => {
                let params: RunParams = Self::read(&arguments)?;
                let game = self.games.get(params.handle).await?;
                // A count of ticks becomes its tick as the call starts, so a repeat runs no
                // further (08-sessions.md, A console's local match starts paused).
                let to = match params.to {
                    RunTo::Ticks(ticks) => {
                        let read = game.ask(ConsoleAsk::Status).await?;
                        let ran = read.tick.map_or(0, |tick| tick + 1);
                        RunTo::Tick((ran + ticks).saturating_sub(1))
                    }
                    to => to,
                };
                let ask = ConsoleAsk::Run(RunAsk {
                    to,
                    wall_seconds: Some(params.wall_seconds.unwrap_or(RUN_WALL_SECONDS)),
                });
                Self::reply_text(&game.ask_until(ask, cancelled).await?)
            }
            ToolName::SimPace => {
                let params: PaceParams = Self::read(&arguments)?;
                let game = self.games.get(params.handle).await?;
                let ask = ConsoleAsk::Pace(PaceAsk {
                    paused: params.paused,
                    speed: params.speed,
                });
                Self::reply_text(&game.ask(ask).await?)
            }
            ToolName::ViewScreenshot => {
                let params: ShotParams = Self::read(&arguments)?;
                let game = self.games.get(params.handle).await?;
                let ask = ConsoleAsk::Screenshot(ShotAsk {
                    region: params.region.unwrap_or(ShotRegion::Window),
                });
                let reply = game.ask(ask).await?;
                let ConsoleAnswer::Screenshot(shot) = reply.answer else {
                    return Ok(CallOutput::text(Self::reply_text(&reply)));
                };
                let tick = reply.tick.map_or_else(
                    || "before tick 0".to_owned(),
                    |tick| format!("after tick {tick}"),
                );
                let [x, y] = shot.origin;
                return Ok(CallOutput {
                    text: format!(
                        "{tick}: {} × {} px of the window's frame from ({x}, {y}), {} image px a \
                         frame px; {}",
                        shot.width,
                        shot.height,
                        shot.scale,
                        Self::pose_text(shot.camera)
                    ),
                    png: Some(shot.png),
                });
            }
            ToolName::ViewCamera => {
                let params: CameraParams = Self::read(&arguments)?;
                let game = self.games.get(params.handle).await?;
                let ask = ConsoleAsk::Camera(params.camera.unwrap_or(CameraAsk::Read));
                Self::reply_text(&game.ask(ask).await?)
            }
        };
        Ok(CallOutput::text(text))
    }

    /// A camera's pose as the agent reads it.
    fn pose_text(pose: CameraPose) -> String {
        let [x, z] = pose.pivot;
        format!(
            "camera at the point ({x}, {z}), yaw {} rad, pitch {} rad, {} m up",
            pose.yaw, pose.pitch, pose.height
        )
    }

    /// The arguments `P` of a call; a failure that says why they do not read.
    fn read<P: DeserializeOwned>(arguments: &JsonObject) -> Result<P, GameFailure> {
        Json::convert(arguments).map_err(GameFailure::Arguments)
    }

    /// A reply as the agent reads it.
    fn reply_text(reply: &Reply) -> String {
        let tick = reply.tick.map_or_else(
            || "before tick 0".to_owned(),
            |tick| format!("after tick {tick}"),
        );
        match &reply.answer {
            ConsoleAnswer::Status(status) => Self::status_text(&tick, status),
            ConsoleAnswer::Units(table) => Self::table_text(table),
            ConsoleAnswer::Ran(stop) => {
                let why = match stop {
                    RunStop::Reached => "it ran its tick",
                    RunStop::Held => "its condition held",
                    RunStop::WallTime => "its wall time passed",
                    RunStop::Cancelled => "a cancel ended it",
                    RunStop::MatchEnded => "the match ended",
                };
                format!("paused {tick}: {why}")
            }
            ConsoleAnswer::Cancelled => "cancelled".to_owned(),
            ConsoleAnswer::Paced(pace) => format!("{tick}, {}", Self::pace_text(*pace)),
            ConsoleAnswer::Screenshot(shot) => format!(
                "{tick}: a screenshot of {} × {} px",
                shot.width, shot.height
            ),
            ConsoleAnswer::Camera(pose) => format!("{tick}: {}", Self::pose_text(*pose)),
            ConsoleAnswer::Refused(refusal) => format!("refused: {refusal:?}"),
        }
    }

    fn pace_text(pace: PaceStatus) -> String {
        let state = if pace.paused { "paused" } else { "playing" };
        format!("{state} at {:?} speed", pace.speed).to_lowercase()
    }

    fn status_text(tick: &str, status: &SessionStatus) -> String {
        let mut text = format!("{tick}, {:?}", status.phase).to_lowercase();
        if let Some(rate) = status.rate {
            write!(text, ", {rate} ticks a second").expect("text writes");
        }
        if let Some(pace) = status.pace {
            write!(text, ", {}", Self::pace_text(pace)).expect("text writes");
        }
        if let Some(hash) = &status.state_hash {
            write!(text, "\nstate hash {hash}").expect("text writes");
        }
        for player in &status.players {
            let resources: Vec<String> = (player.resources.iter())
                .map(|(name, amount)| format!("{name} {amount}"))
                .collect();
            let resources = if resources.is_empty() {
                "no resources".to_owned()
            } else {
                resources.join(", ")
            };
            write!(text, "\nplayer slot {}: {resources}", player.player_slot).expect("text writes");
        }
        let failures = &status.script_failures;
        write!(text, "\nscript failures: {}", failures.count).expect("text writes");
        if let Some(latest) = &failures.latest {
            write!(
                text,
                ", the latest at tick {}, {}: {}",
                latest.tick, latest.hook, latest.error
            )
            .expect("text writes");
        }
        text
    }

    /// A table, with the rows it left when it is cut, and how to read them (02-engine-core.md, A
    /// table comes in pages).
    fn table_text(table: &UnitTable) -> String {
        let mut text = table.text.trim_end().to_owned();
        if table.shown < table.selected {
            let end = table.first + table.shown;
            write!(
                text,
                "\nrows {} to {end} of {}",
                table.first, table.selected
            )
            .expect("text writes");
            if end < table.selected {
                let left = table.selected - end;
                write!(text, "; {left} more from first = {end}").expect("text writes");
            }
            text.push_str("; narrow the units by player_slot, team, type, ids or state");
        }
        text
    }

    fn events_text(events: &[ConsoleEvent]) -> String {
        if events.is_empty() {
            return "no event".to_owned();
        }
        let mut text = String::new();
        for event in events {
            let tick = event
                .tick
                .map_or_else(String::new, |tick| format!("tick {tick} "));
            let level = format!("{:?}", event.level).to_lowercase();
            write!(
                text,
                "{tick}{level} {}: {}",
                event.target,
                event.message.as_deref().unwrap_or("")
            )
            .expect("text writes");
            for (name, value) in &event.fields {
                let value = match value {
                    FieldValue::Bool(value) => value.to_string(),
                    FieldValue::Integer(value) => value.to_string(),
                    FieldValue::Unsigned(value) => value.to_string(),
                    FieldValue::Float(value) => value.to_string(),
                    FieldValue::Text(value) => value.clone(),
                };
                write!(text, " {name}={value}").expect("text writes");
            }
            text.push('\n');
        }
        text
    }
}

impl ServerHandler for GameTools {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(INSTRUCTIONS)
            .with_server_info(Implementation::new("campfire", env!("CARGO_PKG_VERSION")))
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        future::ready(Ok(ListToolsResult::with_all_items(tools())))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        ToolName::parse(name).map(tool_of)
    }

    /// Calls the tool the request names; a failure is the call's result, which says what to
    /// change, and an unknown tool a JSON-RPC error, with the names nearest it.
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let Some(name) = ToolName::parse(&request.name) else {
            let names = ToolName::ALL.into_iter().map(ToolName::name);
            let nearest = NearestNames::of(&request.name, names);
            let nearest: Vec<&str> = nearest.iter().collect();
            return Err(ErrorData::invalid_params(
                format!("no tool {}; the nearest are {nearest:?}", request.name),
                None,
            ));
        };
        let arguments = request.arguments.unwrap_or_default();
        let result = match self.call(name, arguments, context.ct.cancelled()).await {
            Ok(output) => {
                let mut content = vec![ContentBlock::text(output.text)];
                content.extend(output.png.map(|png| ContentBlock::image(png, "image/png")));
                CallToolResult::success(content)
            }
            Err(failure) => {
                let text = ErrorReport::of(&failure).to_string();
                CallToolResult::error(vec![ContentBlock::text(text)])
            }
        };
        Ok(CallToolResponse::Complete(result))
    }
}

#[cfg(test)]
mod tests;
