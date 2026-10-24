# Endpoint holds

How long a UDP server's endpoint holds its lock, for which its frame or its IO thread waits, measured 2026-10-20 on the `structural-hardening` branch, which master took as one change, at three states of it: before a hold bounded its handshakes; after a hold starts at most one handshake; and after a hold takes one batch of work, datagrams and due packet sessions together.

## Machine and method

- **Machine:** Intel Core i9-13980HX, Linux, x86-64-v3, a laptop on mains power.
- **Build:** the bench profile, `cargo drive bench atomic/udp_server`, Criterion's 10 samples of each case.
- **The server:** `net-udp`'s test server, a raw Ed25519 key, the default packet session setup (a keep-alive each 1 s), an admission that limits no address, on the emulated network's driven clock, each client a sealed packet session on a lossless link of its own.
- **A case:** the longest hold of one step, timed around each lock and unlock by the plain host's `HoldClock`, with the IO's reads and sends outside it. The step's other work runs untimed.
  - `datagrams_<n>`: a pass over one 64-byte payload from each of 160 clients, or from each of `n` when fewer, a frame after the last batch, with the ACKs of the last batch due.
  - `keep_alives_<n>`: a pass that sends each of `n` links its keep-alive, all due together.
  - `frame_<n>`: a frame with nothing new, its two steps over every link it carries.
  - `handshakes`: a pass over 160 clients' `Initial`s, each past its Retry, which starts their handshakes.
  - `handshake_ends`: a pass over the ends of 160 handshakes, which opens their packet sessions.

## Results

| Case | Before | One handshake | One batch |
| --- | ---: | ---: | ---: |
| `datagrams_64_links` | 17.20 µs | 16.49 µs | 16.53 µs |
| `datagrams_256_links` | 62.72 µs | 59.83 µs | 36.61 µs |
| `datagrams_1024_links` | 105.74 µs | 94.90 µs | 43.01 µs |
| `datagrams_4096_links` | 164.13 µs | 129.78 µs | 63.65 µs |
| `keep_alives_64_links` | 11.80 µs | 10.96 µs | 11.68 µs |
| `keep_alives_256_links` | 51.64 µs | 49.45 µs | 28.90 µs |
| `keep_alives_1024_links` | 249.98 µs | 228.25 µs | 35.76 µs |
| `keep_alives_4096_links` | 1.394 ms | 1.337 ms | 68.31 µs |
| `frame_64_links` | 0.96 µs | 0.88 µs | 0.90 µs |
| `frame_256_links` | 4.21 µs | 3.72 µs | 3.84 µs |
| `frame_1024_links` | 31.05 µs | 26.48 µs | 32.35 µs |
| `frame_4096_links` | 179.09 µs | 161.60 µs | 164.51 µs |
| `handshakes` | 9.678 ms | 75.62 µs | 76.01 µs |
| `handshake_ends` | 300.12 µs | 313.06 µs | 304.63 µs |

- **A handshake's start** costs 58 µs: a second run before the change gave 9.292 ms for 160, as the TLS server's first flight, its key share and its signature, is written under the hold. Its end costs 1.9 µs.
- **The datagrams and the timers** cost what the design says a pass costs: their own count, 0.8 µs a datagram and 0.33 µs a due link at 4,096 links.
- **The cases a start does not reach** moved between the two runs. Two more runs after the change gave 131.57 µs and 133.09 µs for `datagrams_4096_links`, 19 % below the run before it, and 165.33 µs and 168.26 µs for `frame_4096_links`: the frame's case moves by about 4 % from run to run, and the cause of the datagrams' drop was not measured.
- **The frame's two steps** walk every link the endpoint carries: 39 ns a link at 4,096.
- **One batch of work.** A hold that routes datagrams and polls due packet sessions together, at most 160, and pops each due timer as it polls it, holds the lock for 68 µs as 4,096 keep-alives come due; it took 299 µs when it moved every due timer out of the heap first, in one hold, and 1.34 ms when it polled them all. A batch of datagrams no longer shares its hold with the ACK timers the last batch left due, so its hold fell to 64 µs at 4,096 links. The longest hold is now a batch of 160 handshakes' ends, 305 µs, as each opens a packet session.

## quinn 0.11.12

Read on 20 October 2026. `Incoming::accept` calls `Endpoint::accept`, which locks the endpoint's state and calls quinn-proto 0.11.19's `Endpoint::accept`; that adds the connection and calls `Connection::handle_first_packet`, which reads the client's first packet and its TLS data. Each accepted connection's first flight is written under one hold of its own, and the endpoint driver's loop, bounded at 160 datagrams (`IO_LOOP_BOUND`), hands out an `Incoming` for each new connection with no TLS read. The driver's receive also stops after 50 µs of work, `RECV_TIME_BOUND` through a `WorkLimiter`, as its comment says that going lower made no difference, a `recvmmsg` batch of 32 taking 30 µs on some systems.
