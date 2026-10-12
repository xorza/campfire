#!/usr/bin/env python3
"""Campfire's driver: the one way to check, test, document and bench the workspace.

Cargo keeps a cache for each shape of a command, its features, profile, mode and flags, so the
driver owns every invocation, and its warm loop keeps exactly those shapes built
(docs/design/02-engine-core.md, The driver). `./drive.py help` lists the commands.
"""

from __future__ import annotations

import enum
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
from collections.abc import Callable
from typing import TextIO
from dataclasses import dataclass, field
from pathlib import Path

SOURCE = Path(__file__).resolve().parent
OWN = SOURCE / "target" / "drive"
LOGS = OWN / "log"
WARM = OWN / "warm"
HOLDERS = OWN / "holders"
LAN_ROOT = OWN / "lan-check"

IN_CI = "GITHUB_ACTIONS" in os.environ
POLL_SECONDS = 0.2
# Below watchexec's `--stop-timeout`, so a pass always stops its own cargo before watchexec
# kills the pass, which would leave that cargo running in its own session.
STOP_SECONDS = 10
PRINTED_LINES = 200

# Clippy's arguments after `--` are part of each unit's fingerprint, so every lint passes these.
DENY = ("--", "-D", "warnings")
LINT = (
    "clippy", "--workspace", "--all-targets", "--all-features", "--locked", "--keep-going",
    "--message-format=short",
)
SHIP = ("clippy", "--workspace", "--locked", "--keep-going", "--message-format=short")
TESTS = ("nextest", "run", "--workspace", "--all-features", "--locked")
QUIET_NEXTEST = (
    "--cargo-quiet", "--show-progress", "none", "--status-level", "fail",
    "--final-status-level", "slow", "--failure-output", "final", "--success-output", "never",
)
LAN_PACKAGES = (
    "campfire-lan-check", "campfire-server", "campfire-client", "campfire-verifier",
)

PROGRESS = re.compile(
    r"^\s*(Compiling|Checking|Documenting|Finished|Fresh|Blocking|Downloading|Downloaded|"
    r"Updating|Locking|Adding|Building|Generated|Running|Doc-tests|Scraping)\s"
)
LIBTEST_PASS = re.compile(r"^(running \d+ tests?|test result: ok\..*)$")
# Nextest's run header and its notices of a test still running: its summary lists each slow test.
NEXTEST_PROGRESS = re.compile(r"^\s*(Nextest run ID .*|Starting \d+ tests? .*|\[>\s*[\d.]+s\] .*|─+)$")
ANSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")
# What changes a fingerprint from outside the workspace's own files.
STAMPED = re.compile(
    r"^(RUSTFLAGS|RUSTC|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|RUSTUP_TOOLCHAIN|"
    r"CARGO_ENCODED_RUSTFLAGS|CARGO_INCREMENTAL|CARGO_BUILD_\w+|CARGO_TARGET_\w+|"
    r"CARGO_PROFILE_\w+|CARGO_UNSTABLE_\w+)$"
)


class Platform:
    """What differs between the OSes the driver runs on: no caller tests the OS."""

    exe_suffix = ""
    cross_target: str | None = None
    # Formatting is the same on every OS, so CI checks it on one.
    formats_in_ci = False
    has_warm_loop = False

    def spawn_group(self, argv: list[str], env: dict[str, str], log: TextIO) -> subprocess.Popen:
        raise NotImplementedError

    def interrupt_group(self, process: subprocess.Popen) -> None:
        raise NotImplementedError

    def kill_group(self, process: subprocess.Popen) -> None:
        raise NotImplementedError

    def alive(self, pid: int) -> bool:
        raise NotImplementedError


class Posix(Platform):
    has_warm_loop = True

    def spawn_group(self, argv: list[str], env: dict[str, str], log: TextIO) -> subprocess.Popen:
        # A session of its own, so a stop reaches cargo and every rustc it started, and only
        # when the pass sends it.
        return subprocess.Popen(
            argv, cwd=SOURCE, env=env, stdin=subprocess.DEVNULL, stdout=log,
            stderr=subprocess.STDOUT, start_new_session=True,
        )

    def interrupt_group(self, process: subprocess.Popen) -> None:
        self._signal_group(process, signal.SIGINT)

    def kill_group(self, process: subprocess.Popen) -> None:
        self._signal_group(process, signal.SIGKILL)

    def alive(self, pid: int) -> bool:
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return False
        except PermissionError:
            return True
        return True

    @staticmethod
    def _signal_group(process: subprocess.Popen, number: signal.Signals) -> None:
        try:
            os.killpg(process.pid, number)
        except ProcessLookupError:
            pass


class Linux(Posix):
    cross_target = "x86_64-pc-windows-gnu"
    formats_in_ci = True


class MacOs(Posix):
    pass


class Windows(Platform):
    exe_suffix = ".exe"


def current_platform() -> Platform:
    if sys.platform.startswith("linux"):
        return Linux()
    if sys.platform == "darwin":
        return MacOs()
    if sys.platform == "win32":
        return Windows()
    raise SystemExit(f"drive.py: {sys.platform} is not a platform Campfire runs on")


PLATFORM = current_platform()


@dataclass
class Step:
    name: str
    argv: list[str]
    env: dict[str, str] = field(default_factory=dict)
    # It runs what the step before it built, so it is skipped when that one failed.
    needs_previous: bool = False

    def log_name(self) -> str:
        return self.name.replace(" ", "-") + ".log"


class Status(enum.Enum):
    OK = "ok"
    FAIL = "FAIL"
    SKIP = "skip"


@dataclass
class Outcome:
    name: str
    status: Status
    seconds: float
    log: Path | None
    printed: int

    def line(self) -> str:
        text = f"{self.name:<12} {self.status.value:<4} {self.seconds:6.1f} s"
        if self.status is Status.FAIL and self.log is not None:
            text += f"  {shown_path(self.log)}"
        return text


def cargo(*args: str) -> list[str]:
    return ["cargo", *args]


def nextest_output() -> tuple[str, ...]:
    return ("--profile", "ci") if IN_CI else QUIET_NEXTEST


def fmt_step() -> Step:
    return Step("fmt", cargo("fmt", "--all"))


def fmt_check_step() -> Step:
    return Step("fmt", cargo("fmt", "--all", "--check"))


def lint_step() -> Step:
    return Step("lint", cargo(*LINT, *DENY))


def ship_step() -> Step:
    # A member that turns `internals` on through a normal dependency, as the Zero Hour check does,
    # unifies it into that crate and the crates below it: with such a member, no build that ships
    # is linted. Its own code is linted by `lint`.
    found = metadata()
    members = {package["name"] for package in found["packages"]}
    excluded = [
        arg
        for package in found["packages"]
        if any(
            dependency["kind"] is None
            and not dependency["optional"]
            and dependency["name"] in members
            and "internals" in dependency["features"]
            for dependency in package["dependencies"]
        )
        for arg in ("--exclude", package["name"])
    ]
    return Step("ship", cargo(*SHIP, *excluded, *DENY))


def tests_step(extra: list[str] | None = None, env: dict[str, str] | None = None) -> Step:
    return Step("tests", cargo(*TESTS, *nextest_output(), *(extra or [])), env or {})


def tests_build_step() -> Step:
    return Step("tests", cargo(*TESTS, "--all-targets", "--no-run"))


def bench_cases_step() -> Step:
    return Step("bench cases", cargo(*TESTS, *nextest_output(), "--bench", "*"))


def doctests_step() -> Step:
    return Step("doc tests", cargo("test", "--doc", "--workspace", "--all-features", "--locked"))


def rustdoc_step() -> Step:
    return Step(
        "rustdoc",
        cargo("doc", "--workspace", "--no-deps", "--all-features", "--locked"),
        {"RUSTDOCFLAGS": "-D warnings"},
    )


def cross_step(target: str) -> Step:
    return Step("cross", cargo(*LINT, "--target", target, *DENY))


def lan_build_step() -> Step:
    # One build of the check and the processes it starts, which then share every feature: the
    # check builds them again with the `CARGO` it is given, and finds every crate fresh.
    packages = [arg for package in LAN_PACKAGES for arg in ("-p", package)]
    return Step("lan build", cargo("build", "--locked", *packages))


def lan_run_step() -> Step:
    target = Path(metadata()["target_directory"])
    check = target / "debug" / f"campfire-lan-check{PLATFORM.exe_suffix}"
    return Step("lan", [str(check), str(LAN_ROOT)], {"CARGO": "cargo"}, needs_previous=True)


def warm_steps() -> list[Step]:
    steps = [lint_step(), tests_build_step(), ship_step(), rustdoc_step()]
    if PLATFORM.cross_target is not None:
        steps.append(cross_step(PLATFORM.cross_target))
    steps.append(lan_build_step())
    return steps


def bench_step(args: list[str]) -> Step:
    # Every run selects the same crates, the ones with a `[[bench]]`, so a case measures the same
    # code whichever cases run: `--workspace` would unify the client's Bevy features into the
    # shared crates, and one `-p` would give its dependencies fewer.
    packages = [
        arg
        for package in metadata()["packages"]
        if any("bench" in target["kind"] for target in package["targets"])
        for arg in ("-p", package["name"])
    ]
    return Step("bench", cargo("bench", "--features", "internals", "--locked", *packages, *args))


def metadata() -> dict:
    found = subprocess.run(
        cargo("metadata", "--format-version", "1", "--no-deps", "--locked"),
        cwd=SOURCE, capture_output=True, text=True,
    )
    if found.returncode != 0:
        print(found.stderr, end="", file=sys.stderr)
        raise SystemExit(1)
    return json.loads(found.stdout)


def shown_path(path: Path) -> str:
    try:
        return path.relative_to(SOURCE).as_posix()
    except ValueError:
        return str(path)


class LineFilter:
    """Keeps what a reader acts on: diagnostics, failures and results, not cargo's progress."""

    def __init__(self) -> None:
        self.last_blank = True

    def keeps(self, line: str) -> bool:
        if PROGRESS.match(line) or LIBTEST_PASS.match(line) or NEXTEST_PROGRESS.match(line):
            return False
        blank = not line.strip()
        if blank and self.last_blank:
            return False
        self.last_blank = blank
        return True


def run_step(step: Step) -> Outcome:
    LOGS.mkdir(parents=True, exist_ok=True)
    log = LOGS / step.log_name()
    keep = LineFilter()
    printed = hidden = 0
    start = time.monotonic()
    if IN_CI:
        print(f"::group::{step.name}")
    with log.open("w", encoding="utf-8") as out:
        try:
            process = subprocess.Popen(
                step.argv, cwd=SOURCE, env=os.environ | step.env, stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, encoding="utf-8",
                errors="replace",
            )
        except FileNotFoundError:
            message = f"{step.argv[0]} is not on PATH, or not built"
            out.write(message + "\n")
            print(message)
            printed, code = 1, 127
        else:
            try:
                for raw in process.stdout:
                    line = ANSI.sub("", raw.rstrip("\r\n"))
                    out.write(line + "\n")
                    if IN_CI:
                        print(raw, end="")
                        printed += 1
                    elif keep.keeps(line):
                        if printed < PRINTED_LINES:
                            print(line)
                            printed += 1
                        else:
                            hidden += 1
                code = process.wait()
            except KeyboardInterrupt:
                process.wait()
                raise
    if IN_CI:
        print("::endgroup::")
    if hidden:
        print(f"... {hidden} more lines in {shown_path(log)}")
    status = Status.OK if code == 0 else Status.FAIL
    return Outcome(step.name, status, time.monotonic() - start, log, printed)


def run_steps(steps: list[Step]) -> int:
    with Holder():
        warn_on_stamp()
        outcomes: list[Outcome] = []
        for step in steps:
            if step.needs_previous and outcomes and outcomes[-1].status is not Status.OK:
                outcome = Outcome(step.name, Status.SKIP, 0.0, None, 0)
            else:
                outcome = run_step(step)
            print(outcome.line())
            outcomes.append(outcome)
    # Diagnostics between the status lines leave them apart: gather them again at the end.
    if len(outcomes) > 1 and any(outcome.printed for outcome in outcomes):
        print()
        for outcome in outcomes:
            print(outcome.line())
    return 0 if all(outcome.status is Status.OK for outcome in outcomes) else 1


class Holder:
    """Marks a running command: the warm loop stops its build and leaves cargo's lock to it."""

    def __enter__(self) -> Holder:
        HOLDERS.mkdir(parents=True, exist_ok=True)
        self.path = HOLDERS / str(os.getpid())
        self.path.write_text("")
        return self

    def __exit__(self, *exception: object) -> None:
        self.path.unlink(missing_ok=True)


def commands_running() -> bool:
    try:
        entries = list(HOLDERS.iterdir())
    except FileNotFoundError:
        return False
    running = False
    for entry in entries:
        if entry.name.isdigit() and PLATFORM.alive(int(entry.name)):
            running = True
        else:
            entry.unlink(missing_ok=True)
    return running


def stamp() -> dict[str, str]:
    found = {name: value for name, value in sorted(os.environ.items()) if STAMPED.match(name)}
    rustc = subprocess.run(["rustc", "-V"], cwd=SOURCE, capture_output=True, text=True)
    found["rustc"] = rustc.stdout.strip() or rustc.stderr.strip()
    return found


def warm_loop_pid() -> int | None:
    if not PLATFORM.has_warm_loop:
        return None
    try:
        pid = int((WARM / "pid").read_text())
    except (FileNotFoundError, ValueError):
        return None
    return pid if PLATFORM.alive(pid) else None


def warn_on_stamp() -> None:
    if warm_loop_pid() is None:
        return
    try:
        warm = json.loads((WARM / "stamp.json").read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        return
    here = stamp()
    differ = sorted(name for name in warm.keys() | here.keys() if warm.get(name) != here.get(name))
    if differ:
        print("warning: the warm loop builds in another environment, so its cache does not serve this command")
        for name in differ:
            print(f"  {name}: warm loop {warm.get(name)!r}, here {here.get(name)!r}")


def build_quietly(step: Step) -> Outcome | None:
    """Builds `step` with its output in the warm loop's log; None when a command took over."""
    logs = WARM / "log"
    logs.mkdir(parents=True, exist_ok=True)
    log = logs / step.log_name()
    start = time.monotonic()
    with log.open("w", encoding="utf-8") as out:
        process = PLATFORM.spawn_group(step.argv, os.environ | step.env, out)
        try:
            while True:
                try:
                    code = process.wait(timeout=POLL_SECONDS)
                    break
                except subprocess.TimeoutExpired:
                    if commands_running():
                        stop_group(process)
                        return None
        except KeyboardInterrupt:
            stop_group(process)
            raise
    status = Status.OK if code == 0 else Status.FAIL
    return Outcome(step.name, status, time.monotonic() - start, log, 0)


def stop_group(process: subprocess.Popen) -> None:
    PLATFORM.interrupt_group(process)
    try:
        process.wait(timeout=STOP_SECONDS)
    except subprocess.TimeoutExpired:
        PLATFORM.kill_group(process)
        process.wait()


def interrupt(number: int, frame: object) -> None:
    raise KeyboardInterrupt


def warm_pass(args: list[str]) -> int:
    refuse_arguments("_pass", args)
    signal.signal(signal.SIGTERM, interrupt)
    WARM.mkdir(parents=True, exist_ok=True)
    (WARM / "stamp.json").write_text(json.dumps(stamp(), indent=1))
    steps = warm_steps()
    try:
        index = 0
        while index < len(steps):
            if commands_running():
                print(f"{clock()} waiting for a command")
                while commands_running():
                    time.sleep(POLL_SECONDS)
            outcome = build_quietly(steps[index])
            if outcome is None:
                print(f"{clock()} stopped for a command")
                index = 0
                continue
            print(f"{clock()} {outcome.line()}")
            index += 1
        print(f"{clock()} warm")
    except KeyboardInterrupt:
        return 130
    return 0


def clock() -> str:
    return time.strftime("%H:%M:%S")


def refuse_arguments(name: str, args: list[str]) -> None:
    if args:
        print(f"drive.py {name} takes no arguments", file=sys.stderr)
        raise SystemExit(2)


def check(args: list[str]) -> int:
    refuse_arguments("check", args)
    return run_steps([fmt_step(), lint_step(), ship_step(), tests_step()])


def test(args: list[str]) -> int:
    split = args.index("--") if "--" in args else len(args)
    own, rest = args[:split], args[split:]
    env = {"CAMPFIRE_BLESS": "1"} if "--bless" in own else {}
    own = [arg for arg in own if arg != "--bless"]
    return run_steps([tests_step(own + rest, env)])


def doc(args: list[str]) -> int:
    refuse_arguments("doc", args)
    return run_steps([doctests_step(), rustdoc_step()])


def bench(args: list[str]) -> int:
    return run_steps([bench_step(args)])


def lan(args: list[str]) -> int:
    refuse_arguments("lan", args)
    return run_steps([lan_build_step(), lan_run_step()])


def zero_hour(args: list[str]) -> int:
    return run_steps([
        Step(
            "zero hour",
            cargo("run", "--release", "--locked", "-p", "campfire-zero-hour-check", "--", *args),
        )
    ])


def gate(args: list[str]) -> int:
    refuse_arguments("gate", args)
    steps = []
    if not IN_CI or PLATFORM.formats_in_ci:
        steps.append(fmt_check_step())
    steps += [
        lint_step(), ship_step(), tests_step(), bench_cases_step(), doctests_step(), rustdoc_step(),
    ]
    # CI checks Windows on Windows itself.
    if not IN_CI and PLATFORM.cross_target is not None:
        steps.append(cross_step(PLATFORM.cross_target))
    # Last, so target/debug holds the LAN check's build of each binary, which CI keeps.
    steps += [lan_build_step(), lan_run_step()]
    return run_steps(steps)


def warm(args: list[str]) -> int:
    refuse_arguments("warm", args)
    if not PLATFORM.has_warm_loop:
        print("drive.py warm runs on Linux and macOS", file=sys.stderr)
        return 1
    running = warm_loop_pid()
    if running is not None:
        print(f"drive.py warm runs already, as process {running}", file=sys.stderr)
        return 1
    watchexec = shutil.which("watchexec")
    if watchexec is None:
        print("drive.py warm needs watchexec on PATH", file=sys.stderr)
        return 1
    WARM.mkdir(parents=True, exist_ok=True)
    (WARM / "pid").write_text(str(os.getpid()))
    try:
        process = subprocess.Popen(
            [
                watchexec, "--restart", "--stop-signal", "SIGINT", "--stop-timeout", "30s",
                "--debounce", "300ms", "--shell=none", "--watch", str(SOURCE),
                "--ignore", "target/**", "--",
                sys.executable, str(Path(__file__).resolve()), "_pass",
            ],
            cwd=SOURCE,
        )
        while True:
            try:
                return process.wait()
            except KeyboardInterrupt:
                # Watchexec takes the same Ctrl-C: it stops the pass, which stops its cargo.
                continue
    finally:
        (WARM / "pid").unlink(missing_ok=True)


COMMANDS: dict[str, tuple[Callable[[list[str]], int], str, str]] = {
    "check": (check, "", "fmt, lint, ship, tests: the check after a change"),
    "test": (test, "[--bless] [nextest args]", "the tests, all or filtered"),
    "doc": (doc, "", "the doc tests, and rustdoc with -D warnings"),
    "bench": (bench, "[cargo bench args]", "the benches of every crate that has one"),
    "lan": (lan, "", "the LAN check"),
    "zero-hour": (zero_hour, "[run root]", "the Zero Hour check, which needs GENERALSZH_DATA"),
    "gate": (gate, "", "every step CI runs"),
    "warm": (warm, "", "builds every lane again on each change, with watchexec"),
}


def usage() -> str:
    lines = ["usage: drive.py <command> [arguments]", ""]
    for name, (_, arguments, summary) in COMMANDS.items():
        lines.append(f"  {(name + ' ' + arguments).strip():<36} {summary}")
    return "\n".join(lines)


def main(argv: list[str]) -> int:
    sys.stdout.reconfigure(line_buffering=True, errors="replace")
    if not argv or argv[0] in ("help", "-h", "--help"):
        print(usage())
        return 0 if argv else 2
    name, args = argv[0], argv[1:]
    if name == "_pass":
        return warm_pass(args)
    if name not in COMMANDS:
        print(f"drive.py: no command {name!r}\n\n{usage()}", file=sys.stderr)
        return 2
    try:
        return COMMANDS[name][0](args)
    except KeyboardInterrupt:
        return 130


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
