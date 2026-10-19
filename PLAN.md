# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Steps

**T1. The toolchain back to stable.** Touches `source/rust-toolchain.toml`. When Rust 1.100 is released, on 2026-11-12, the pin moves from the dated beta to stable 1.100, as its comment says.
- **Check:** `./drive.py gate` passes on the stable toolchain.

## Foundations

- **F1. No load scenario for the QUIC thread.** Cause: the harness's links model delay and loss, not a rate, and no bench drives the thread with many connections at once. Limits: what a burst, a flood of handshakes or a full send queue costs the thread and the frame is not known, so no pass bounds its work. Items: the four Quic items of the issue log.
