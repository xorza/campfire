//! A session log as text, to read: its terms, each segment and its checkpoint, each tick's
//! entries, the seed reveal and the result.

use std::io::{self, Write};
use std::path::Path;

use campfire_common::Tick;
use campfire_protocol::{
    HeldStart, LeaveReason, LogEntry, PlayerSlotStart, ServerInput, SessionLog, SessionResult,
};
use campfire_store::InputFile;

use crate::log_text::error::ShowError;

pub(crate) mod error;

/// Writes the session log of the file `path` into `out` as text, as `show_log` writes it.
pub fn show_file(path: &Path, out: &mut dyn Write) -> Result<(), ShowError> {
    let bytes = InputFile::read(path, SessionLog::MAX_FILE_LEN).map_err(ShowError::Read)?;
    let log = SessionLog::decode(&bytes).map_err(ShowError::Decode)?;
    show_log(&log, out).map_err(ShowError::Write)
}

/// Writes `log` into `out` as text, a line for each thing it holds: its session, its terms and
/// each player slot's start; each segment, with its checkpoint's tick, state hash and snapshot;
/// each tick of an entry, with each entry, a packet's inputs by stamp and length, a server input
/// by its kind; the entries the next tick takes; the seeds revealed; and the result. A tick of no
/// entry has no line.
pub fn show_log(log: &SessionLog, out: &mut dyn Write) -> io::Result<()> {
    let terms = &log.header().terms;
    writeln!(out, "session {}", log.session_id())?;
    writeln!(
        out,
        "release {}, map {}, {} ticks a second",
        terms.release, terms.map, terms.tick_hz
    )?;
    for (slot, start) in log.header().player_slots.iter().enumerate() {
        let start = match start {
            PlayerSlotStart::Player(delegation) => {
                format!("player of delegation {}", delegation.id())
            }
            PlayerSlotStart::Bot => "bot".to_owned(),
            PlayerSlotStart::Open => "open".to_owned(),
        };
        writeln!(out, "player slot {slot}: {start}")?;
    }
    let HeldStart {
        segment: mut held,
        tick: first,
    } = log.first_held();
    write_segment(log, held, first, out)?;
    for tick in first.get()..log.next_tick().get() {
        let tick = Tick::new(tick);
        if tick > first
            && let Some(next) = log.segment_starting(tick)
            && next != held
        {
            held = next;
            write_segment(log, held, tick, out)?;
        }
        let entries = log
            .tick_entries(tick)
            .expect("a tick the log sealed is held whole");
        write_entries(&format!("tick {tick}"), entries, out)?;
    }
    write_entries("next tick", log.open_entries(), out)?;
    match log.revealed_seeds() {
        Some(_) => writeln!(out, "seeds revealed through segment {}", log.segment())?,
        None => writeln!(out, "seeds not revealed")?,
    }
    match log.result() {
        Some(SessionResult {
            tick,
            outcome,
            state_hash,
        }) => writeln!(out, "result {outcome:?} at tick {tick}, state {state_hash}"),
        None => writeln!(out, "no result"),
    }
}

/// Writes the line of segment `segment`, which starts at `tick`, and its checkpoint's record.
fn write_segment(
    log: &SessionLog,
    segment: u32,
    tick: Tick,
    out: &mut dyn Write,
) -> io::Result<()> {
    match log.checkpoint_at(tick) {
        Some(checkpoint) => writeln!(
            out,
            "segment {segment} from tick {tick}: checkpoint state {}, snapshot {}",
            checkpoint.state_hash, checkpoint.snapshot
        ),
        None => writeln!(out, "segment {segment} from tick {tick}"),
    }
}

/// Writes the line `title` and a line for each of `entries`, when it holds one.
fn write_entries<'l>(
    title: &str,
    entries: impl Iterator<Item = LogEntry<'l>>,
    out: &mut dyn Write,
) -> io::Result<()> {
    let mut entries = entries.peekable();
    if entries.peek().is_none() {
        return Ok(());
    }
    writeln!(out, "{title}")?;
    for entry in entries {
        match entry {
            LogEntry::Packet(packet) => {
                let inputs: Vec<String> = packet
                    .inputs()
                    .map(|input| {
                        format!(
                            "player slot {} at tick {}, {} bytes",
                            input.player_slot.get(),
                            input.stamp,
                            input.payload.len()
                        )
                    })
                    .collect();
                writeln!(out, "  packet: {}", inputs.join("; "))?;
            }
            LogEntry::Server(input) => writeln!(out, "  server: {}", server_text(&input.input()))?,
        }
    }
    Ok(())
}

/// A server input's kind and player slot, as text.
fn server_text(input: &ServerInput<'_>) -> String {
    match input {
        ServerInput::Bot {
            player_slot,
            payload,
        } => format!(
            "bot of player slot {}, {} bytes",
            player_slot.get(),
            payload.len()
        ),
        ServerInput::Join {
            player_slot,
            delegation,
        } => format!(
            "join of player slot {} by delegation {}",
            player_slot.get(),
            delegation.id()
        ),
        ServerInput::Renew {
            player_slot,
            delegation,
        } => format!(
            "renewal of player slot {} by delegation {}",
            player_slot.get(),
            delegation.id()
        ),
        ServerInput::Leave {
            player_slot,
            reason,
            becomes,
        } => {
            let reason = match reason {
                LeaveReason::Asked => "asked",
                LeaveReason::Grace => "past its grace",
            };
            format!(
                "leave of player slot {}, {reason}, which becomes {becomes:?}",
                player_slot.get()
            )
        }
        ServerInput::Connected { player_slot } => {
            format!("link of player slot {} back", player_slot.get())
        }
        ServerInput::Disconnected { player_slot } => {
            format!("link of player slot {} down", player_slot.get())
        }
    }
}
