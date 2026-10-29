use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};

use bevy_ecs::resource::Resource;
use campfire_codec::Json;
use campfire_common::ErrorReport;
use campfire_console::{
    ConsoleAnswer, ConsoleEvent, ConsoleRefusal, EventLevel, FieldValue, GameRole, Output, Reply,
    Request, RequestId,
};
use campfire_log::{LogLevel, LogRecord, LogValue};
use campfire_logging::LogTap;
use campfire_threads::{Job, Worker};
use serde::Deserialize;
use tracing::warn;

use crate::console::log_seen::LogSeen;

/// A game's console on its standard streams (02-engine-core.md, A game's console is its standard
/// streams): its opening line first; a reader on a thread of its own queues each line of the
/// standard input, and each event of the log goes to the standard output as it is logged, beside
/// the replies. Its input's end stops its game as a stop's signal does (02-engine-core.md, A game
/// stops as its console's input ends).
#[derive(Resource, Debug)]
pub struct ConsoleStdio {
    lines: Mutex<Receiver<String>>,
    ended: AtomicBool,
    seen: Arc<Mutex<LogSeen>>,
    _reader: Worker,
}

/// The id of a line that reads as no request, when it holds one.
#[derive(Debug, Deserialize)]
struct UnreadId {
    id: RequestId,
}

/// Why a lock of what the log's tap saw fails: a panic as it was held.
pub(crate) const SEEN_POISONED: &str = "no code panics holding what the log's tap saw";

impl ConsoleStdio {
    /// Opens the console of a game of `role` on the standard streams, its log's events taken
    /// from `tap`.
    pub fn open(role: GameRole, tap: &LogTap) -> Self {
        Self::write(&Output::opening(role));
        let seen = Arc::new(Mutex::new(LogSeen::default()));
        tap.start({
            let seen = Arc::clone(&seen);
            move |record| {
                seen.lock().expect(SEEN_POISONED).see(record);
                Self::write(&Output::Event(Self::event(record)));
            }
        });
        let (queue, lines) = mpsc::channel();
        let reader = Worker::start_left(Job::Console, move || {
            for line in io::stdin().lock().lines() {
                let Ok(line) = line else {
                    break;
                };
                if queue.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            lines: Mutex::new(lines),
            ended: AtomicBool::new(false),
            seen,
            _reader: reader,
        }
    }

    /// What the log's tap saw, which a match's console reads.
    pub const fn seen(&self) -> &Arc<Mutex<LogSeen>> {
        &self.seen
    }

    /// Whether the input ended.
    pub fn is_ended(&self) -> bool {
        self.ended.load(Ordering::Relaxed)
    }

    /// Adds each line the reader queued that reads as a request to `requests`, in its order; a
    /// line that reads as none is refused at once, when it names its id, and logged when it names
    /// none. The input's end, once its last line is read, ends the console.
    pub fn take(&self, requests: &mut Vec<Request>) {
        let lines = self
            .lines
            .lock()
            .expect("no code panics holding the console's lines");
        loop {
            match lines.try_recv() {
                Ok(line) => {
                    if let Some(request) = Self::read(&line) {
                        requests.push(request);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.ended.store(true, Ordering::Relaxed);
                    break;
                }
            }
        }
    }

    /// Writes `reply` to the standard output.
    pub fn send(&self, reply: Reply) {
        Self::write(&Output::Reply(reply));
    }

    /// The request `line` holds; none for a line that reads as no request, which is refused,
    /// with no tick, when it names its id, and logged when it names none.
    fn read(line: &str) -> Option<Request> {
        let error = match Json::parse::<Request>(line) {
            Ok(request) => return Some(request),
            Err(error) => error,
        };
        let report = ErrorReport::of(&error);
        if let Ok(UnreadId { id }) = Json::parse::<UnreadId>(line) {
            Self::write(&Output::Reply(Reply {
                id,
                tick: None,
                answer: ConsoleAnswer::Refused(ConsoleRefusal::Unread {
                    error: report.to_string(),
                }),
            }));
        } else {
            warn!(error = %report, "a line of the console reads as no request, of no id");
        }
        None
    }

    /// The console's event of `record`: its message and its tick apart from its other fields,
    /// each a message of text and a tick of no sign.
    fn event(record: &LogRecord) -> ConsoleEvent {
        let mut fields: BTreeMap<String, FieldValue> = record
            .fields
            .iter()
            .map(|(name, value)| {
                let value = match value {
                    &LogValue::Bool(value) => FieldValue::Bool(value),
                    &LogValue::Integer(value) => FieldValue::Integer(value),
                    &LogValue::Unsigned(value) => FieldValue::Unsigned(value),
                    &LogValue::Float(value) => FieldValue::Float(value),
                    LogValue::Text(text) => FieldValue::Text(text.clone()),
                };
                ((*name).to_owned(), value)
            })
            .collect();
        let message = match fields.remove("message") {
            Some(FieldValue::Text(message)) => Some(message),
            Some(other) => {
                fields.insert("message".to_owned(), other);
                None
            }
            None => None,
        };
        let tick = match fields.get("tick") {
            Some(&FieldValue::Unsigned(tick)) => Some(tick),
            Some(&FieldValue::Integer(tick)) => u64::try_from(tick).ok(),
            _ => None,
        };
        if tick.is_some() {
            fields.remove("tick");
        }
        ConsoleEvent {
            level: match record.level {
                LogLevel::Trace => EventLevel::Trace,
                LogLevel::Debug => EventLevel::Debug,
                LogLevel::Info => EventLevel::Info,
                LogLevel::Warn => EventLevel::Warn,
                LogLevel::Error => EventLevel::Error,
            },
            target: record.target.to_owned(),
            tick,
            message,
            fields,
        }
    }

    /// Writes `output` to the standard output, a line, whole, beside the lines other threads
    /// write; a write the parent no longer reads changes nothing, as its input ends with it.
    fn write(output: &Output) {
        let mut line = Json::write(output);
        line.push('\n');
        let mut out = io::stdout().lock();
        out.write_all(line.as_bytes())
            .and_then(|()| out.flush())
            .ok();
    }
}

#[cfg(test)]
mod tests {
    use campfire_console::ConsoleAsk;

    use super::*;

    #[test]
    fn a_record_becomes_its_event_with_its_message_and_tick_apart() {
        let text = |text: &str| LogValue::Text(text.to_owned());
        let record = |level, fields| LogRecord {
            level,
            target: "campfire_runner",
            fields,
        };
        let failed = record(
            LogLevel::Warn,
            vec![
                ("message", text("a call failed")),
                ("tick", LogValue::Unsigned(7)),
                ("unit", LogValue::Unsigned(4)),
                ("hook", text("on_hit")),
                ("late", LogValue::Bool(true)),
                ("share", LogValue::Float(0.5)),
                ("behind", LogValue::Integer(-2)),
            ],
        );
        assert_eq!(
            ConsoleStdio::event(&failed),
            ConsoleEvent {
                level: EventLevel::Warn,
                target: "campfire_runner".to_owned(),
                tick: Some(7),
                message: Some("a call failed".to_owned()),
                fields: BTreeMap::from([
                    ("behind".to_owned(), FieldValue::Integer(-2)),
                    ("hook".to_owned(), FieldValue::Text("on_hit".to_owned())),
                    ("late".to_owned(), FieldValue::Bool(true)),
                    ("share".to_owned(), FieldValue::Float(0.5)),
                    ("unit".to_owned(), FieldValue::Unsigned(4)),
                ]),
            }
        );
        // A message that is no text and a tick below 0 stay fields.
        let odd = record(
            LogLevel::Info,
            vec![
                ("message", LogValue::Integer(3)),
                ("tick", LogValue::Integer(-1)),
            ],
        );
        let odd = ConsoleStdio::event(&odd);
        assert_eq!((odd.message, odd.tick), (None, None));
        assert_eq!(
            odd.fields,
            BTreeMap::from([
                ("message".to_owned(), FieldValue::Integer(3)),
                ("tick".to_owned(), FieldValue::Integer(-1)),
            ])
        );
    }

    #[test]
    fn a_line_reads_as_its_request_or_as_none() {
        assert_eq!(
            ConsoleStdio::read(r#"{"id":1,"ask":"status"}"#),
            Some(Request {
                id: RequestId(1),
                ask: ConsoleAsk::Status,
            })
        );
        assert_eq!(ConsoleStdio::read(r#"{"id":3,"ask":"what"}"#), None);
        assert_eq!(ConsoleStdio::read("no request"), None);
    }
}
