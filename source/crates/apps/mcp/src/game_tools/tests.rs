use std::env;
use std::env::consts::EXE_SUFFIX;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use campfire_console::{FailedScriptCall, Output, PlayerStatus, ScriptFailureCount, SessionPhase};
use campfire_store::InputFile;
use campfire_store::internals::TestDir;
use campfire_test_packages::TestMode;
use rmcp::model::{ClientConfig, ProtocolVersion};
use rmcp::service::RunningService;
use rmcp::{ClientHandler, ClientLifecycleMode, ClientServiceExt, RoleClient, ServiceExt};
use tokio::io;

use crate::game_kind::BuildProfile;

use super::*;

/// A client of the tests, of a revision of MCP's that keeps the `initialize` handshake.
#[derive(Debug, Clone)]
struct Initializing(ProtocolVersion);

impl ClientHandler for Initializing {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::default().with_protocol_version(self.0.clone())
    }
}

/// How a test's client opens: by the `initialize` handshake of 2025-11-25, or the stateless
/// discovery of 2026-07-28.
#[derive(Debug, Clone, Copy)]
enum Revision {
    Initialize,
    Stateless,
}

/// The workspace's target directory, where the build of the tests put the binaries, as the
/// driver's build leaves `CARGO_TARGET_DIR` to its default; the tests' own binaries may lie in a
/// build directory apart.
fn target() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target")
}

/// A client of a server of `games`, over a pipe in the process, opened by `revision`.
async fn client(games: Arc<Games>, revision: Revision) -> RunningService<RoleClient, Initializing> {
    let (server, client) = io::duplex(1 << 20);
    tokio::spawn(async move {
        let running = GameTools::new(games).serve(server).await.unwrap();
        running.waiting().await.ok();
    });
    let handler = Initializing(ProtocolVersion::V_2025_11_25);
    match revision {
        Revision::Initialize => handler.serve(client).await.unwrap(),
        Revision::Stateless => handler
            .serve_with_lifecycle(
                client,
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                },
            )
            .await
            .unwrap(),
    }
}

/// The text of the call `name` of `arguments`, and whether it failed.
async fn call(
    client: &RunningService<RoleClient, Initializing>,
    name: &'static str,
    arguments: &str,
) -> (String, bool) {
    let arguments: JsonObject = Json::parse(arguments).unwrap();
    let result = client
        .call_tool(CallToolRequestParams::new(name).with_arguments(arguments))
        .await
        .unwrap();
    let [ContentBlock::Text(text)] = result.content.as_slice() else {
        panic!("{result:?}");
    };
    (text.text.clone(), result.is_error == Some(true))
}

#[tokio::test(flavor = "current_thread")]
async fn the_tools_list_in_one_order_with_their_hints_under_both_revisions() {
    let names = [
        "game_start",
        "game_stop",
        "game_events",
        "sim_status",
        "sim_units",
        "sim_run",
        "sim_pace",
        "view_screenshot",
        "view_camera",
    ];
    let reads = ["game_events", "sim_status", "sim_units", "view_screenshot"];
    let repeats = [
        "game_stop",
        "game_events",
        "sim_status",
        "sim_units",
        "sim_run",
        "sim_pace",
        "view_screenshot",
        "view_camera",
    ];
    for revision in [Revision::Initialize, Revision::Stateless] {
        let games = Arc::new(Games::new(target(), None));
        let client = client(games, revision).await;
        let listed = client.list_all_tools().await.unwrap();
        let listed_names: Vec<&str> = listed.iter().map(|tool| tool.name.as_ref()).collect();
        assert_eq!(listed_names, names, "{revision:?}");
        for tool in &listed {
            let hints = tool.annotations.as_ref().unwrap();
            let name = tool.name.as_ref();
            assert!(tool.title.is_some(), "{name}");
            assert_eq!(hints.read_only_hint, Some(reads.contains(&name)), "{name}");
            assert_eq!(
                hints.idempotent_hint,
                Some(repeats.contains(&name)),
                "{name}"
            );
            assert_eq!(hints.destructive_hint, Some(false), "{name}");
            let meta = tool.meta.as_ref().unwrap();
            let size: u32 = Json::convert(&meta.0["anthropic/maxResultSizeChars"]).unwrap();
            assert_eq!(size, RESULT_CHARS);
            assert_eq!(
                tool.input_schema.get("type").and_then(|kind| kind.as_str()),
                Some("object")
            );
        }
        // The schemas derive from the console's types: a run's end and a table's units.
        let schema = |name: &str| {
            Json::write(
                &listed
                    .iter()
                    .find(|tool| tool.name == name)
                    .unwrap()
                    .input_schema,
            )
        };
        assert!(
            schema("sim_run").contains("script_failure"),
            "{}",
            schema("sim_run")
        );
        assert!(
            schema("sim_units").contains("player_slot"),
            "{}",
            schema("sim_units")
        );
        // No game runs, and a call of one says so; an unknown tool names the nearest.
        assert_eq!(
            call(&client, "sim_status", "{}").await,
            ("no game runs; start one with game_start".to_owned(), true)
        );
        let unknown = client
            .call_tool(CallToolRequestParams::new("sim_statu"))
            .await
            .unwrap_err()
            .to_string();
        assert!(unknown.contains("sim_status"), "{unknown}");
        client.cancel().await.unwrap();
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_status_of_a_server_under_its_console_and_its_exit_fail_their_calls() {
    let test_dir = TestDir::new();
    let target = target();
    let server = target
        .join("debug")
        .join(format!("campfire-server{EXE_SUFFIX}"));
    let games = Arc::new(Games::new(target, None));
    let client = client(Arc::clone(&games), Revision::Stateless).await;
    let lane = TestMode::Lane.dir();
    let args: Vec<OsString> = vec![
        "--data".into(),
        test_dir.path("data").into(),
        "--no-port-map".into(),
        lane.into(),
        "127.0.0.1:0".into(),
    ];
    let launched = games.launch(&server, &args).await.unwrap();
    assert_eq!(launched.opening.role, GameRole::Server);
    assert_eq!(launched.opening.fingerprint, Output::fingerprint());
    // The only game needs no handle; its lobby has no tick.
    let (status, failed) = call(&client, "sim_status", "{}").await;
    assert!(
        !failed && status.starts_with("before tick 0, lobby"),
        "{status}"
    );
    let named = format!(r#"{{"handle":{}}}"#, launched.handle);
    assert_eq!(call(&client, "sim_status", &named).await.0, status);
    let (unknown, failed) = call(&client, "sim_status", r#"{"handle":99}"#).await;
    assert!(
        failed && unknown.contains("no game has the handle 99"),
        "{unknown}"
    );
    // A run is a local match's: the dedicated server refuses it.
    let (refused, failed) = call(&client, "sim_run", r#"{"to":{"tick":5}}"#).await;
    assert!(failed && refused.contains("NotLocal"), "{refused}");
    // Its stop closes its input, and it exits as a stop's signal makes it.
    let (stopped, failed) = call(&client, "game_stop", "{}").await;
    assert!(!failed && stopped.ends_with("exit code 130"), "{stopped}");
    assert!(call(&client, "sim_status", "{}").await.1);

    // A server that fails its load exits after its opening: a call fails with its status.
    let broken: Vec<OsString> = vec![
        "--data".into(),
        test_dir.path("broken").into(),
        "--no-port-map".into(),
        test_dir.path("no-mode").into(),
        "127.0.0.1:0".into(),
    ];
    games.launch(&server, &broken).await.unwrap();
    let (exited, failed) = call(&client, "sim_status", "{}").await;
    assert!(
        failed && exited.starts_with("the game exited, exit code 1"),
        "{exited}"
    );
    client.cancel().await.unwrap();
}

/// The arguments by which a test's binary runs one of its own tests, `name`, as a game the
/// server starts, the console's flag after them taken as filters that match no test.
fn fake_game(name: &str) -> Vec<OsString> {
    let test = format!("game_tools::tests::{name}");
    vec![
        test.into(),
        "--exact".into(),
        "--nocapture".into(),
        "--".into(),
    ]
}

/// Whether the test runs as a game a test started, with the console's flag.
fn runs_as_game() -> bool {
    env::args().any(|arg| arg == "--console")
}

#[test]
fn fake_game_of_other_types() {
    if runs_as_game() {
        println!(r#"{{"opening":{{"role":"server","fingerprint":1}}}}"#);
        std::io::stdin().lock().lines().for_each(drop);
    }
}

#[test]
fn fake_game_that_strays() {
    if !runs_as_game() {
        return;
    }
    println!("a dependency's print");
    println!("{}", Json::write(&Output::opening(GameRole::Server)));
    for line in std::io::stdin().lock().lines() {
        let request: campfire_console::Request = Json::parse(&line.unwrap()).unwrap();
        println!("another print, between two replies");
        let status = SessionStatus {
            phase: SessionPhase::Running,
            rate: Some(30),
            state_hash: None,
            pace: None,
            players: vec![PlayerStatus {
                player_slot: 0,
                resources: BTreeMap::from([("gold".to_owned(), 7)]),
            }],
            script_failures: ScriptFailureCount {
                count: 1,
                latest: Some(FailedScriptCall {
                    tick: 3,
                    unit: None,
                    hook: "on_think".to_owned(),
                    error: "no".to_owned(),
                }),
            },
        };
        let reply = Reply {
            id: request.id,
            tick: Some(41),
            answer: ConsoleAnswer::Status(status),
        };
        println!("{}", Json::write(&Output::Reply(reply)));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_game_of_other_types_is_refused_and_a_stray_line_is_read_past() {
    let games = Arc::new(Games::new(target(), None));
    let exe = env::current_exe().unwrap();
    let refused = games
        .launch(&exe, &fake_game("fake_game_of_other_types"))
        .await;
    let Err(GameFailure::Fingerprint { server, .. }) = refused else {
        panic!("{refused:?}");
    };
    assert_eq!(server, Output::fingerprint());

    let client = client(Arc::clone(&games), Revision::Initialize).await;
    games
        .launch(&exe, &fake_game("fake_game_that_strays"))
        .await
        .unwrap();
    assert_eq!(
        call(&client, "sim_status", "{}").await,
        (
            "after tick 41, running, 30 ticks a second\nplayer slot 0: gold 7\nscript failures: \
             1, the latest at tick 3, on_think: no"
                .to_owned(),
            false
        )
    );
    client.cancel().await.unwrap();
    games.stop_all().await;
}

#[test]
fn a_cut_table_names_the_rows_it_left() {
    let table = |first, shown| UnitTable {
        text: "tick 9\nid\n 0\n 1\n\n".to_owned(),
        selected: 5,
        first,
        shown,
    };
    assert_eq!(
        GameTools::table_text(&table(1, 2)),
        "tick 9\nid\n 0\n 1\nrows 1 to 3 of 5; 2 more from first = 3; narrow the units by \
         player_slot, team, type, ids or state"
    );
    // A page of the last rows names none more; a whole table names nothing.
    assert_eq!(
        GameTools::table_text(&table(3, 2)),
        "tick 9\nid\n 0\n 1\nrows 3 to 5 of 5; narrow the units by player_slot, team, type, ids \
         or state"
    );
    assert_eq!(GameTools::table_text(&table(0, 5)), "tick 9\nid\n 0\n 1");
}

/// The project's `.mcp.json`, as Claude Code reads it.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpFile {
    mcp_servers: BTreeMap<String, McpServer>,
}

#[derive(Debug, serde::Deserialize)]
struct McpServer {
    command: String,
}

/// The driver's command that builds the MCP server, as the workspace's manifest names it.
#[derive(Debug, serde::Deserialize)]
struct Manifest {
    workspace: Workspace,
}

#[derive(Debug, serde::Deserialize)]
struct Workspace {
    metadata: WorkspaceMetadata,
}

#[derive(Debug, serde::Deserialize)]
struct WorkspaceMetadata {
    drive: Drive,
}

#[derive(Debug, serde::Deserialize)]
struct Drive {
    commands: BTreeMap<String, DriveCommand>,
}

#[derive(Debug, serde::Deserialize)]
struct DriveCommand {
    steps: Vec<DriveStep>,
}

#[derive(Debug, serde::Deserialize)]
struct DriveStep {
    build: Option<String>,
}

#[test]
fn the_projects_mcp_file_names_the_server_the_driver_builds() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mcp = InputFile::read_text(&source.join("../.mcp.json"), 1 << 16).unwrap();
    let mcp: McpFile = Json::parse(&mcp).unwrap();
    // Claude Code sets the project's directory in the server's environment alone, so the
    // command defaults it, and the server finds the workspace by its own path.
    assert_eq!(
        mcp.mcp_servers["campfire"].command,
        "${CLAUDE_PROJECT_DIR:-.}/source/target/release-dev/campfire-mcp"
    );
    let manifest = InputFile::read_text(&source.join("Cargo.toml"), 1 << 20).unwrap();
    let manifest: Manifest = campfire_codec::Toml::parse(&manifest).unwrap();
    let commands = &manifest.workspace.metadata.drive.commands;
    let build = |name: &str| commands[name].steps[0].build.clone().unwrap();
    assert_eq!(build("mcp"), "build --profile release-dev -p campfire-mcp");
    // The games it starts build in the profiles whose directories it reads.
    assert_eq!(
        GameKind::Client.binary(Path::new("t"), BuildProfile::Fast),
        Path::new("t/release-dev").join(format!("campfire-client{EXE_SUFFIX}"))
    );
    assert_eq!(
        GameKind::Server.binary(Path::new("t"), BuildProfile::Checked),
        Path::new("t/debug").join(format!("campfire-server{EXE_SUFFIX}"))
    );
    assert!(build(BuildProfile::Fast.command()).contains("--profile release-dev"));
    assert!(!build(BuildProfile::Checked.command()).contains("--profile"));
}
