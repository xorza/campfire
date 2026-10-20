# Time

How the stack answers "what time is it".

- **One clock per app, owned by the stack.** Every networking read (the packet sessions' send,
  receive, ACK and loss times and their timers, the rate bucket, fragment cleanup, the transport's
  heartbeat, the remote timeline) takes the app's `Clock`. Refused: the global `mock_instant` clock,
  shared by every thread of a test binary, so tests could not run in parallel under `cargo test` and
  two apps could not run at different rates; and `Time<Real>` everywhere, whose frame-start instant
  moves a send stamp up to one frame from the send in production and biases RTT and the sync target.
- **An external driver drives Bevy's time, and the `Frame` source follows it.** A harness drives
  an app's time the one way Bevy defines, `TimeUpdateStrategy`, and a `Frame` clock reads the
  app's `Time<Real>` of the frame, so the network clock cannot disagree with the tick clock.
  Refused: a `Manual` source a harness sets beside `TimeUpdateStrategy` (the shape of Campfire's
  `Os::Clock`), a second knob to keep equal to the first; and a `dyn` time source, an open set with
  no second real source.
- **The default source follows how the app's time runs.** `Auto` reads the OS clock while the
  app's `TimeUpdateStrategy` is `Automatic` and the frame's `Time<Real>` once a harness drives it,
  so an app whose ticks follow a driven time can never measure its network on the OS clock.
  Refused: `Os` as the default, which gave a harness that forgot to pick `Frame` exactly the
  run-to-run RTTs the clock exists to remove.
- **The stack's own `Instant`, a `Duration` since the app's first update.** It shares the origin
  of `Time<Real>::elapsed()`, so both sources count from one moment and two runs of one seed print
  the same times. Refused: `bevy_platform::time::Instant`, which a driven clock can only offset
  from a real OS instant, and which `net-core` re-exported.
- **A real-socket IO runs on `Os`.** An IO whose thread waits on sockets needs the OS time; a
  driven clock cannot drive a socket's waits.
- **Ticks stay on Bevy's clocks.** `Time<Fixed>` and `Time<Virtual>` drive ticks and timelines,
  so a game speed changes ticks and leaves the network clock real.
- **The tick length is a rate, and the timestep is its period.** The rate is `math`'s `TickRate`,
  ticks over whole seconds, which the timelines read as `TimelineRate`. The timeline plugin alone
  writes `Time<Fixed>`'s timestep, the period rounded once to the nanosecond, and a conversion
  between ticks and time computes the exact rational and rounds once. Practice: Netcode for
  Entities' `SimulationTickRate`, an integer in hertz. Refused: a `Duration` per tick beside the
  timestep, a second truth whose rounding made 3 ticks at 60 Hz 50_000_001 ns.
- **One tick rate type, two resources.** `math` holds the one `TickRate`, ticks over a whole
  number of seconds, so a paced rate, half of 21 Hz among them, stays exact, its tick rounded to
  the nearest nanosecond, half the error of rounding down. The sim reads the match's rate,
  `MatchRate`, which only the terms set, and the timelines their own, `TimelineRate`, which
  `SetTickRate` sets, so a sped-up local match keeps its sim's rate, and no resource is named
  `TickRate`. Refused: whole hertz, which holds no half of an odd rate; a float rate, which is not
  exact; a type of the stack's own beside the sim's, two types of one quantity.
- **One number type.** A tick span is a `Num`, the overstep a `Num` in `[0, 1)`, a span that
  cannot be negative a checked newtype of one, and the emulator's probabilities `Num`s, as a test's
  data gives them. A `Num` holds the ranges of tick spans, and its step of 2⁻²⁴ tick, 1 ns at
  60 Hz, is finer than the 2⁻¹⁶ the wire carries. Refused: the `fixed` crate beside `Num`, two
  fixed-point types with two rounding rules.
- **A rollback replays each tick at the period it ran with.** Prediction keeps the period of each
  tick in the rollback window, and the rewind takes each one off the fixed time. Refused: one
  timestep for the whole window, which a `SetTickRate` inside it breaks.
- **An emulated network has its own wire clock.** Deliveries are due on it, the OS time or a time
  the harness sets, and the receiving app stamps them with its own `Clock`. The apps' clocks
  cannot schedule a wire: each counts from its own first update, and a harness runs them at
  different rates and stalls them. Refused: due times in the sender's clock compared with the
  receiver's.
- **Conversions are exact.** A `Duration` becomes ticks through its nanoseconds into fixed point,
  a `PositiveTickSpan` becomes a duration unsigned, and every narrowing goes through `TryFrom`.
  Timestamps are `Duration`s, never `f32` seconds, and an EMA updates on the small difference.
- **The tick family saturates,** at the range of its own type, instead of wrapping, and an
  instant plus an instant does not exist. The remote estimate starts at the remote's tick and
  keeps only a small correction as its offset, which a signed offset from the local origin would
  saturate past 2^31 ticks.
- **An RTT sample is exact.** The packet session takes one from each ACK of a packet that asked for
  one, less the time its largest waited at the peer, which the ACK says ([Each ACK says how long its
  largest waited](packet-session.md)). Practice: QUIC's `ack_delay` (RFC 9002 §5), ENet's
  `receivedSentTime`. Refused: ping messages, which a frame's wait delays and which cost bytes on a
  busy link.
- **Two estimates of one link keep two promises.** The packet session keeps RFC 9002's for its fast
  timers; sync's `LinkEstimator` takes one of the packet session's samples each 100 ms, its gains
  tuned for that rate, for a stable input delay. Refused: one estimate for both, whose gains suit
  one promise and break the other.
- **An unknown estimate is `None`,** not zero: the timelines wait for the handshake's samples.
- **An input leads by one margin for each reason.** On loopback the local timeline leads the
  server by 2 ticks: one so that an input reaches the server before the server simulates its tick,
  which reads inputs after its tick advances, and one for the sync deadband. The input timeline's
  `jitter_margin` defaults to 0, a tail margin above the measured jitter that interpolation keeps
  at 1. Practice: Netcode for Entities' `TargetCommandSlack` of 2 ticks. Refused: a second tick
  for the input's arrival, which paid the same reason twice.
- **A client behind steps, a client ahead slews.** The input timeline steps to its objective once a
  deficit passes the deadband, as a late input costs more than latency, and the interpolation
  timeline slews, as a step shows as a jump ([LAN late input](../../research/lan-late-input.md)).
  Practice: NTP steps a large offset and slews a small one. Refused: a lead the server acknowledges,
  which comes back only after an input, as a client sends one only with an order.
