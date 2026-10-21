use campfire_log::internals::round_trip;

use super::*;

#[test]
fn the_event_reads_back_what_it_logs() {
    let tick = Tick::new(30);
    let print = ScriptPrinted::of(
        tick,
        ScriptPrint {
            kind: PrintKind::Print,
            text: "wave 2",
        },
    );
    let debug = ScriptPrinted::of(
        tick,
        ScriptPrint {
            kind: PrintKind::Debug {
                line: Some(4),
                column: None,
            },
            text: "#{a: 1}",
        },
    );
    assert_eq!((print.debug, print.line, print.column), (false, None, None));
    assert_eq!(
        (debug.debug, debug.line, debug.column),
        (true, Some(4), None)
    );
    round_trip(&print);
    round_trip(&debug);
}
