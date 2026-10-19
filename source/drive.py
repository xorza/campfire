#!/usr/bin/env python3
"""Campfire's driver: the one way to check, test and bench the workspace.

Cargo keeps a cache for each shape of a command, its features, profile, mode and flags, so the
driver owns every invocation (docs/design/02-engine-core.md, The driver). `./drive.py help` lists
the commands.
"""

from __future__ import annotations

import collections
import enum
import functools
import hashlib
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
from collections.abc import Callable, Iterable, Mapping
from dataclasses import dataclass, field, replace
from pathlib import Path
from typing import IO

SOURCE = Path(__file__).resolve().parent
# The one folder outside the workspace that its tests read: the design documents, which tests
# compare to the code.
DOCS = SOURCE.parent / "docs"
# How code names the design documents' folder: in a path, `docs/`, or as a part, `"docs"`.
DOCS_NAMED = re.compile(r'docs/|"docs"')
OWN = SOURCE / "target" / "drive"
LOGS = OWN / "log"
# One JSON line for each step a command ran, so the checks' times are measured, not assumed.
HISTORY = OWN / "history.jsonl"
# For each step, the tree it last passed on.
PASSES = OWN / "passes.json"
LAN_ROOT = OWN / "lan-check"

IN_CI = "GITHUB_ACTIONS" in os.environ
# A step prints its first lines and its last: the summaries come last, nextest's list of the
# failed tests among them.
HEAD_LINES = 150
TAIL_LINES = 50

# Clippy's arguments after `--` are part of each unit's fingerprint, so every lint passes these.
DENY = ("--", "-D", "warnings")
# Cargo's messages as JSON, each diagnostic rendered short: the driver tells a lint from code that
# does not compile, and prints each diagnostic once. The format is in no fingerprint.
JSON = "--message-format=json-diagnostic-short"
LINT = (
    "clippy", "--workspace", "--all-targets", "--all-features", "--locked", "--keep-going", JSON,
)
SHIP = ("clippy", "--locked", "--keep-going", JSON)
TESTS = (
    "nextest", "run", "--workspace", "--all-features", "--locked",
    "--cargo-message-format", "json-diagnostic-short",
)
QUIET_NEXTEST = (
    "--show-progress", "none", "--status-level", "fail",
    "--final-status-level", "slow", "--failure-output", "final", "--success-output", "never",
)
# The one test that compares each workspace package to its `package.index`, and writes the index
# again from the package's files when the run blesses: it reads the packages' directories, which
# `package-disk` alone does, by the format `package` holds.
INDEX_PACKAGE = "campfire-package-disk"
INDEX_TEST = (
    f"package(={INDEX_PACKAGE}) & "
    "test(=package_dir::tests::every_workspace_package_holds_the_index_of_its_files)"
)
# The crate whose tests read the files of every crate, the workspace's rules: a change in any
# crate reaches it. A new test that reads other crates' files goes in it.
WORKSPACE_READERS = frozenset(("campfire-rules",))
# The LAN check and the binaries it runs, its `Binaries::NAMES`, which its test holds the same.
LAN_BINARIES = (
    "campfire-lan-check", "campfire-server", "campfire-client", "campfire-verifier",
)

# The binaries a release ships, each stripped, with its debug info beside it.
SHIPPED_BINARIES = (
    "campfire-client", "campfire-server", "campfire-verifier", "campfire-import",
)

# The lines a step does not print: cargo's progress; nextest's run header and its notices of a
# test still running, as its summary lists each slow test; cargo's tallies of the diagnostics a
# step prints one by one, and nextest's of its failures.
HIDDEN = re.compile(
    r"^\s*(Compiling|Checking|Documenting|Finished|Fresh|Blocking|Downloading|Downloaded|"
    r"Updating|Locking|Adding|Building|Generated|Running|Scraping)\s"
    r"|^\s*(Nextest run ID .*|Starting \d+ tests? .*|\[>\s*[\d.]+s\] .*|─+)$"
    r"|^(warning: `[^`]+` \(.*\) generated \d+ warnings?.*"
    r"|error: could not compile `[^`]+` \(.*\) due to \d+ previous errors?.*"
    r"|warning: build failed, waiting for other jobs to finish\.\.\."
    r"|error: test run failed)$"
)
# Nextest's count of the tests it ran, which goes on the step's line.
NEXTEST_SUMMARY = re.compile(r"^\s*Summary \[\s*[\d.]+s\] (.*)$")
# A unit cargo had to build.
BUILT = re.compile(r"^\s*(Compiling|Checking|Documenting)\s")
# What changes a step's result from outside the working files: the toolchain's settings, cargo's,
# and the tests' own.
STEP_ENV = re.compile(r"^(RUST\w*|CARGO\w*|CAMPFIRE_\w+)$")
ANSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")
# Rustc's code of an error that is no lint; a lint's code is its name.
ERROR_CODE = re.compile(r"^E\d{4}$")
# The note that ends a warning of one of cargo's own lints, as `unused_dependencies`, which cargo
# prints as text and `-D warnings` does not reach. One lane may print a warning another does not,
# so each lane that builds counts it.
CARGO_LINT = re.compile(r"^\s*= note: `cargo::[\w-]+` is set to `warn`")


class DebugSplit(enum.Enum):
    """How a platform's release build keeps its debug info beside its binary
    (02-engine-core.md, A release keeps its debug info beside its binaries)."""

    # `objcopy` copies it into `<bin>.debug`, then strips the binary and links that file by name.
    OBJCOPY = enum.auto()
    # Cargo packs it into `<bin>.dSYM`, and `strip -S` drops it from the binary.
    DSYM = enum.auto()
    # The linker writes it into `<crate>.pdb` already, which goes beside the binary.
    PDB = enum.auto()

    def build_env(self) -> dict[str, str]:
        if self is DebugSplit.DSYM:
            return {"CARGO_PROFILE_RELEASE_SPLIT_DEBUGINFO": "packed"}
        return {}

    def steps(self, name: str, built: Path, out: Path) -> list[Step]:
        """The steps that put the binary `built` of the package `name` in `out`, stripped, and
        its debug info beside it."""
        shipped = out / built.name
        def step(what: str, *argv: str | Path) -> Step:
            return Step(f"{what} {name}", Kind.PROGRAM, tuple(map(str, argv)), needs_previous=True)
        if self is DebugSplit.OBJCOPY:
            debug = out / f"{built.name}.debug"
            return [
                step("debug", "objcopy", "--only-keep-debug", built, debug),
                step("strip", "objcopy", "--strip-debug", f"--add-gnu-debuglink={debug}", built, shipped),
            ]
        if self is DebugSplit.DSYM:
            return [
                step("strip", "strip", "-S", "-o", shipped, built),
                step("debug", *COPY_TREE, built.with_name(f"{built.name}.dSYM"), out / f"{built.name}.dSYM"),
            ]
        pdb = built.with_name(f"{name.replace('-', '_')}.pdb")
        return [step("binary", *COPY_FILE, built, shipped), step("debug", *COPY_FILE, pdb, out / pdb.name)]


# Copies with no shell, the same on every OS.
COPY_FILE = (sys.executable, "-c", "import shutil, sys; shutil.copy2(sys.argv[1], sys.argv[2])")
COPY_TREE = (
    sys.executable, "-c",
    "import shutil, sys; shutil.rmtree(sys.argv[2], True); shutil.copytree(sys.argv[1], sys.argv[2])",
)
MAKE_DIR = (
    sys.executable, "-c", "import pathlib, sys; pathlib.Path(sys.argv[1]).mkdir(parents=True, exist_ok=True)",
)


@dataclass(frozen=True)
class Platform:
    """What differs between the OSes the driver runs on: no caller tests the OS."""

    exe_suffix: str = ""
    cross_target: str | None = None
    # Formatting is the same on every OS, so CI checks it on one.
    formats_in_ci: bool = False
    debug_split: DebugSplit = DebugSplit.OBJCOPY


def current_platform() -> Platform:
    if sys.platform.startswith("linux"):
        return Platform(cross_target="x86_64-pc-windows-gnu", formats_in_ci=True)
    if sys.platform == "darwin":
        return Platform(debug_split=DebugSplit.DSYM)
    if sys.platform == "win32":
        return Platform(exe_suffix=".exe", debug_split=DebugSplit.PDB)
    raise SystemExit(f"drive.py: {sys.platform} is not a platform Campfire runs on")


PLATFORM = current_platform()


class Kind(enum.Enum):
    """What a step runs, which says what its output holds and what its failure means."""

    # `cargo fmt`, which builds nothing.
    FORMAT = enum.auto()
    # A build or a lint, which fails only through the diagnostics it reports.
    CHECK = enum.auto()
    # A nextest run: a build, then the tests.
    TESTS = enum.auto()
    # A program whose output is its own: a bench, a check binary.
    PROGRAM = enum.auto()

    def diagnoses(self) -> bool:
        return self in (Kind.CHECK, Kind.TESTS)


@dataclass(frozen=True)
class Step:
    name: str
    kind: Kind
    argv: tuple[str, ...]
    env: Mapping[str, str] = field(default_factory=dict)
    # It runs what the step before it made, so it is skipped when that one did not pass.
    needs_previous: bool = False
    # The command asks for a run each time: no pass is reused, and no nextest run narrowed, but
    # the pass it makes is kept, so a later command on the same tree reuses it.
    always_runs: bool = False
    # The members whose tests a nextest step runs, None for every member.
    scope: frozenset[str] | None = None
    # The step whose kept pass this one's reuse follows: it runs whenever that one would run, and
    # keeps no pass of its own.
    follows: Step | None = None

    def narrowed(self, members: Iterable[str]) -> Step:
        # Nextest unites its filters, so a narrowed step can hold no filter of its own.
        assert "-E" not in self.argv, f"{self.name} narrows a filter of its own"
        expression = " | ".join(f"package(={member})" for member in sorted(members))
        return replace(self, argv=(*self.argv, "-E", expression, "--no-tests=pass"))

    def log(self) -> Path:
        return LOGS / (self.name.replace(" ", "-") + ".log")


class Status(enum.Enum):
    OK = "ok"
    FAIL = "FAIL"
    SKIP = "skip"


@dataclass(frozen=True)
class Outcome:
    name: str
    status: Status
    seconds: float = 0.0
    log: Path | None = None
    printed: int = 0
    built: int = 0
    note: str = ""
    # It found code that does not compile, so no later step builds it.
    broken: bool = False
    # It passed on these inputs before, so it did not run.
    reused: bool = False

    def line(self) -> str:
        parts = [f"{self.name:<12} {self.status.value:<4} {self.seconds:6.1f} s"]
        if self.status is Status.FAIL and self.log is not None:
            parts.append(shown_path(self.log))
        if self.note:
            parts.append(self.note)
        return "  ".join(parts)


@dataclass(frozen=True)
class Workspace:
    """The workspace's members as `cargo metadata` gives them."""

    folders: Mapping[str, Path]
    # The members that depend on each member, by a dependency of any kind, target or feature.
    dependents: Mapping[str, frozenset[str]]
    benches: frozenset[str]
    # The members with an `internals` feature, which a bench crate needs on, but the top test
    # crate, all of whose code is test code.
    internals: frozenset[str]
    target: Path

    def reach(self, changed: Iterable[str]) -> frozenset[str]:
        """The members whose tests a change in `changed` can change: they and every member that
        depends on them, and `WORKSPACE_READERS`."""
        return frozenset(closure(changed, self.dependents) | WORKSPACE_READERS)

    def holder(self, path: Path) -> str | None:
        """The member that holds `path`: the deepest whose folder holds it; None for none."""
        holders = [name for name, folder in self.folders.items() if path.is_relative_to(folder)]
        return max(holders, key=lambda name: len(self.folders[name].parts), default=None)


def closure(start: Iterable[str], edges: Mapping[str, Iterable[str]]) -> set[str]:
    """`start` and every name its `edges` reach."""
    found = set(start)
    queue = list(found)
    while queue:
        for next_name in edges[queue.pop()]:
            if next_name not in found:
                found.add(next_name)
                queue.append(next_name)
    return found


@functools.cache
def workspace() -> Workspace:
    found = subprocess.run(
        cargo("metadata", "--format-version", "1", "--no-deps", "--locked"),
        cwd=SOURCE, capture_output=True, text=True,
    )
    if found.returncode != 0:
        print(found.stderr, end="", file=sys.stderr)
        raise SystemExit(1)
    metadata = json.loads(found.stdout)
    packages = metadata["packages"]
    names = {package["name"] for package in packages}
    dependents: dict[str, set[str]] = {name: set() for name in names}
    for package in packages:
        for dependency in package["dependencies"]:
            if dependency["name"] in names:
                dependents[dependency["name"]].add(package["name"])
    return Workspace(
        folders={package["name"]: Path(package["manifest_path"]).parent for package in packages},
        dependents={name: frozenset(found) for name, found in dependents.items()},
        benches=frozenset(
            package["name"]
            for package in packages
            if any("bench" in target["kind"] for target in package["targets"])
        ),
        internals=frozenset(
            package["name"] for package in packages if "internals" in package["features"]
        ),
        target=Path(metadata["target_directory"]),
    )


def cargo(*args: str) -> tuple[str, ...]:
    return ("cargo", *args)


def nextest_output() -> tuple[str, ...]:
    return ("--profile", "ci") if IN_CI else QUIET_NEXTEST


def index_step() -> Step:
    return Step(
        "index", Kind.TESTS, cargo(*TESTS, *nextest_output(), "-E", INDEX_TEST),
        {"CAMPFIRE_BLESS": "1"}, scope=frozenset((INDEX_PACKAGE,)),
    )


def fmt_step() -> Step:
    return Step("fmt", Kind.FORMAT, cargo("fmt", "--all"))


def fmt_check_step() -> Step:
    return Step("fmt", Kind.FORMAT, cargo("fmt", "--all", "--check"))


def lint_step() -> Step:
    return Step("lint", Kind.CHECK, cargo(*LINT, *DENY))


def ship_step() -> Step:
    # The workspace's default members, which leave out each member that turns `internals` on
    # through a normal dependency, as the harness does, or depends on one that does, as the Zero
    # Hour check does: with such a member, no build that ships is linted. Its own code is linted
    # by `lint`, and `campfire-rules` holds the list.
    return Step("ship", Kind.CHECK, cargo(*SHIP, *DENY))


def tests_step(
    extra: Iterable[str] = (), env: Mapping[str, str] | None = None, always_runs: bool = False,
) -> Step:
    argv = cargo(*TESTS, *nextest_output(), *extra)
    return Step("tests", Kind.TESTS, argv, env or {}, always_runs=always_runs)


def cross_step(target: str) -> Step:
    return Step("cross", Kind.CHECK, cargo(*LINT, "--target", target, *DENY))


def lan_build_step() -> Step:
    # The check and the processes it starts, beside it, as they ship: each package by `-p`, with
    # no feature beyond those they ship with, so no `internals` gate is in what the check runs. One
    # invocation builds the four, so the server and the verifier carry the client's Bevy features,
    # as one build costs CI less. It runs whenever the check runs, as the tests' build writes the
    # server and the verifier beside it with every feature.
    packages = [arg for package in LAN_BINARIES for arg in ("-p", package)]
    return Step(
        "lan build", Kind.CHECK,
        cargo("build", "--locked", "--bins", *packages, JSON),
        follows=lan_run_step(),
    )


def lan_run_step() -> Step:
    check = workspace().target / "debug" / f"campfire-lan-check{PLATFORM.exe_suffix}"
    return Step(
        "lan", Kind.PROGRAM, (str(check), str(LAN_ROOT)), needs_previous=True,
    )


def bench_step(args: list[str]) -> Step:
    # Every run selects the same crates, the ones with a `[[bench]]`, so a case measures the same
    # code whichever cases run: `--workspace` would unify the client's Bevy features into the
    # shared crates, and one `-p` would give its dependencies fewer.
    benches = sorted(workspace().benches)
    packages = [arg for name in benches for arg in ("-p", name)]
    features = ",".join(f"{name}/internals" for name in benches if name in workspace().internals)
    argv = cargo("bench", "--features", features, "--locked", *packages, *args)
    return Step("bench", Kind.PROGRAM, argv, always_runs=True)


def bench_check_step() -> Step:
    # Criterion's `--test` runs each case once, so a bench that no longer runs fails the gate; the
    # same selection as `bench`'s, so the two share one build.
    return replace(bench_step(["--", "--test"]), name="benches", always_runs=False)


def shown_path(path: Path) -> str:
    try:
        return path.relative_to(SOURCE).as_posix()
    except ValueError:
        return str(path)


class StepOutput:
    """What a step prints: diagnostics, failures and results, not cargo's progress, and each error
    diagnostic once in a command. Its log holds every line, each diagnostic short."""

    def __init__(self, kind: Kind, file: IO[str], shown: set[str]) -> None:
        self.kind = kind
        self.file = file
        # The error diagnostics the command printed, shared by its steps.
        self.shown = shown
        # A crate's library and its tests report one diagnostic twice.
        self.logged: set[str] = set()
        # The reader of standard output and the reader of standard error share the counts.
        self.lock = threading.Lock()
        self.last_blank = True
        self.printed = 0
        self.hidden = 0
        self.tail: collections.deque[str] = collections.deque(maxlen=TAIL_LINES)
        self.built = 0
        self.errors = 0
        self.repeated = 0
        self.summary = ""
        # Errors that are no lint: the code does not compile.
        self.hard = 0
        # What ended the reader of standard output, raised again on the step's thread.
        self.failure: BaseException | None = None

    def run(self, step: Step) -> int:
        try:
            process = subprocess.Popen(
                step.argv, cwd=SOURCE, env=os.environ | dict(step.env), stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, encoding="utf-8",
                errors="replace",
            )
        except FileNotFoundError:
            self.take_line(f"{step.argv[0]} is not on PATH, or not built")
            return 127
        reader = threading.Thread(target=self.read_stdout, args=(process.stdout,), daemon=True)
        reader.start()
        try:
            for raw in process.stderr:
                self.take_line(raw)
            reader.join()
            if self.failure is not None:
                raise self.failure
            return process.wait()
        except BaseException:
            process.terminate()
            process.wait()
            raise

    def read_stdout(self, pipe: IO[str]) -> None:
        try:
            for raw in pipe:
                self.take_stdout(raw)
        except BaseException as failure:
            self.failure = failure
            # The child blocks on a full pipe, and its step with it, unless the pipe drains.
            for _ in pipe:
                pass

    def take_stdout(self, raw: str) -> None:
        if self.kind.diagnoses() and raw.startswith("{"):
            try:
                message = json.loads(raw)
            except json.JSONDecodeError:
                message = None
            if isinstance(message, dict) and "reason" in message:
                if message["reason"] == "compiler-message":
                    self.take_diagnostic(message["message"])
                return
        self.take_line(raw)

    def take_line(self, raw: str) -> None:
        line = ANSI.sub("", raw.rstrip("\r\n"))
        with self.lock:
            self.file.write(line + "\n")
            if BUILT.match(line):
                self.built += 1
            if self.kind.diagnoses() and CARGO_LINT.match(line):
                self.errors += 1
            summary = NEXTEST_SUMMARY.match(line)
            if summary:
                self.summary = summary[1]
            if IN_CI:
                print(raw.rstrip("\r\n"))
            elif not (summary or HIDDEN.match(line)):
                self.show(line)

    def take_diagnostic(self, diagnostic: dict) -> None:
        rendered = (diagnostic.get("rendered") or diagnostic["message"]).rstrip("\n")
        with self.lock:
            if rendered in self.logged:
                return
            self.logged.add(rendered)
            self.file.write(rendered + "\n")
            if IN_CI:
                print(rendered)
            # The lints print each warning, as an error.
            if not diagnostic["level"].startswith("error"):
                return
            self.errors += 1
            code = (diagnostic.get("code") or {}).get("code")
            if code is None or ERROR_CODE.match(code):
                self.hard += 1
            if rendered in self.shown:
                self.repeated += 1
            elif not IN_CI:
                self.shown.add(rendered)
                self.show(rendered)

    def show(self, line: str) -> None:
        blank = not line.strip()
        if blank and self.last_blank:
            return
        self.last_blank = blank
        if self.printed < HEAD_LINES:
            print(line)
            self.printed += 1
        else:
            self.tail.append(line)
            self.hidden += 1

    def finish(self, log: Path) -> None:
        omitted = self.hidden - len(self.tail)
        if omitted:
            print(f"... {omitted} more lines in {shown_path(log)}")
        for line in self.tail:
            print(line)
        self.printed += len(self.tail)

    def notes(self) -> list[str]:
        found = [self.summary] if self.summary else []
        if self.errors:
            text = f"{self.errors} error{'s' if self.errors != 1 else ''}"
            found.append(text + (f", {self.repeated} shown above" if self.repeated else ""))
        return found


def run_step(step: Step, shown: set[str], note: str = "") -> Outcome:
    log = step.log()
    start = time.monotonic()
    if IN_CI:
        print(f"::group::{step.name}")
    # Written aside and moved in whole, so a run beside this one leaves each log whole.
    written = log.with_name(f"{log.stem}-{os.getpid()}.tmp")
    try:
        with written.open("w", encoding="utf-8") as file:
            output = StepOutput(step.kind, file, shown)
            code = output.run(step)
    finally:
        written.replace(log)
    if IN_CI:
        print("::endgroup::")
    output.finish(log)
    status = Status.OK if code == 0 and not output.errors else Status.FAIL
    # A lint that fails with no diagnostic failed before rustc ran: no build gets further.
    broken = output.hard > 0 or (
        status is Status.FAIL and step.kind is Kind.CHECK and not output.errors
    )
    notes = [text for text in (note, *output.notes()) if text]
    return Outcome(
        step.name, status, time.monotonic() - start, log, output.printed, output.built,
        "; ".join(notes), broken,
    )


def run_steps(command: str, steps: list[Step], reuse: bool = False) -> int:
    """Runs `steps`, each after the one before it. Outside CI, the first step that fails skips the
    rest, as a fix comes before what follows; CI runs on past failures, so one run lists each: a
    step that builds is skipped there once one found code that does not compile, and a step that
    needs the one before it when that one did not pass. With `reuse`, outside CI, each step runs
    only as `Passes` allows."""
    passes = Passes() if reuse and not IN_CI else None
    shown: set[str] = set()
    outcomes: list[Outcome] = []
    broken: str | None = None
    failed: str | None = None
    for step in steps:
        if failed is not None:
            outcome = Outcome(step.name, Status.SKIP, note=f"{failed} did not pass")
        elif broken is not None and step.kind is not Kind.FORMAT:
            note = f"{broken} found code that does not compile"
            outcome = Outcome(step.name, Status.SKIP, note=note)
        elif step.needs_previous and outcomes and outcomes[-1].status is not Status.OK:
            outcome = Outcome(step.name, Status.SKIP, note=f"{outcomes[-1].name} did not pass")
        elif passes is not None:
            outcome = passes.run(step, shown)
        else:
            outcome = run_step(step, shown)
        if outcome.broken and broken is None:
            broken = outcome.name
        if outcome.status is Status.FAIL and not IN_CI:
            failed = outcome.name
        print(outcome.line())
        record(command, outcome)
        outcomes.append(outcome)
    # Diagnostics between the status lines leave them apart: gather them again at the end, under a
    # heading, so no one reads them as a second run.
    if len(outcomes) > 1 and any(outcome.printed for outcome in outcomes):
        print("\nsummary")
        for outcome in outcomes:
            print(outcome.line())
    return 0 if all(outcome.status is Status.OK for outcome in outcomes) else 1


def record(command: str, outcome: Outcome) -> None:
    entry = {
        "at": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "command": command,
        "pid": os.getpid(),
        "step": outcome.name,
        "status": outcome.status.value,
        "seconds": round(outcome.seconds, 1),
        "built": outcome.built,
        "reused": outcome.reused,
        "note": outcome.note,
    }
    with HISTORY.open("a", encoding="utf-8") as history:
        history.write(json.dumps(entry) + "\n")


def git(*args: str, env: Mapping[str, str] | None = None) -> str | None:
    found = subprocess.run(
        ["git", *args], cwd=SOURCE, env=os.environ | dict(env or {}), capture_output=True,
        text=True,
    )
    return found.stdout if found.returncode == 0 else None


@functools.cache
def git_index() -> Path | None:
    index = git("rev-parse", "--path-format=absolute", "--git-path", "index")
    return Path(index.strip()) if index else None


@functools.cache
def repository() -> Path | None:
    top = git("rev-parse", "--show-toplevel")
    return Path(top.strip()) if top else None


def working_tree() -> str | None:
    """The id of git's tree of the working files as they stand, tracked and new alike, as
    `.gitignore` leaves them; None outside git. It is written through a copy of the index, the
    process's own, so the index stays as it is and concurrent runs share no file."""
    index = git_index()
    if index is None:
        return None
    copy = OWN / f"tree-index-{os.getpid()}"
    try:
        try:
            shutil.copyfile(index, copy)
        except FileNotFoundError:
            copy.unlink(missing_ok=True)
        env = {"GIT_INDEX_FILE": str(copy)}
        if git("add", "--all", "--", ":/", env=env) is None:
            return None
        tree = git("write-tree", env=env)
        return tree.strip() if tree else None
    finally:
        copy.unlink(missing_ok=True)


@dataclass(frozen=True)
class Change:
    """The change from the tree a step last passed on to the tree it would run on."""

    # The changed files the workspace reads: those in it, and the design documents.
    inputs: tuple[Path, ...]
    # The members whose tests the change can change; None for every member, when a changed input
    # is in no member or is a manifest, whose features unify into members that do not depend on
    # it.
    reach: frozenset[str] | None

    @staticmethod
    def between(since: str, tree: str) -> Change | None:
        """None when git cannot compare them, as when `since` is gone."""
        top = repository()
        changed = git("diff", "--name-only", "--no-renames", "-z", since, tree)
        if top is None or changed is None:
            return None
        inputs = tuple(
            path
            for path in (top / name for name in changed.split("\0") if name)
            if path.is_relative_to(SOURCE) or path.is_relative_to(DOCS)
        )
        return Change(inputs, Change.reach_of(inputs))

    @staticmethod
    def reach_of(inputs: tuple[Path, ...]) -> frozenset[str] | None:
        owners = set()
        documents = False
        for path in inputs:
            if path.is_relative_to(DOCS):
                documents = True
                continue
            holder = workspace().holder(path)
            if path.name == "Cargo.toml" or holder is None:
                return None
            owners.add(holder)
        reach = workspace().reach(owners)
        return reach | Change.documents_reach() if documents else reach

    @staticmethod
    @functools.cache
    def documents_reach() -> frozenset[str]:
        """The members whose tests a change of a design document can change: each member whose
        tests name the documents' folder, and each member whose other code names it, with every
        member that depends on it."""
        folders = workspace().folders
        tests: set[str] = set()
        code: set[str] = set()
        paths = {path for folder in folders.values() for path in folder.rglob("*.rs")}
        for path in sorted(paths):
            if not DOCS_NAMED.search(path.read_text(errors="replace")):
                continue
            holder = workspace().holder(path)
            assert holder is not None, f"{path} is in a member's folder"
            within = path.relative_to(folders[holder]).parts
            (tests if "tests" in within or path.name == "tests.rs" else code).add(holder)
        return frozenset(tests) | (workspace().reach(code) if code else frozenset())


class Passes:
    """The tree each step last passed on, with its command, its environment and the tools.

    A step does not run again when no file the workspace reads changed since that tree. A nextest
    step runs only the tests of the members the change reaches (`Workspace.reach`), and does not
    run when it reaches none of its own: the other tests passed on that tree, and nothing they
    read changed. A pass is kept only when no file changed while the step ran."""

    def __init__(self) -> None:
        self.trees = Passes.load()
        self.tree = working_tree()

    @staticmethod
    def load() -> dict[str, str]:
        try:
            return json.loads(PASSES.read_text())
        except (FileNotFoundError, json.JSONDecodeError):
            return {}

    def run(self, step: Step, shown: set[str]) -> Outcome:
        tree = self.tree
        if tree is None:
            return run_step(step, shown)
        followed = step.follows or step
        key = step_key(followed)
        last = self.trees.get(key)
        change = None if last is None else Change.between(last, tree)
        run, note = step, ""
        if change is not None and not step.always_runs:
            if not change.inputs:
                since = "it" if step.follows is None else followed.name
                return self.reuse(step, key, tree, f"no input changed since {since} passed")
            if step.kind is Kind.TESTS and change.reach is not None:
                scope = step.scope if step.scope is not None else frozenset(workspace().folders)
                reached = scope & change.reach
                if not reached:
                    return self.reuse(step, key, tree, "no change reaches its tests")
                if reached != scope:
                    run = step.narrowed(reached)
                    note = f"the tests of {len(reached)} of {len(scope)} crates"
        outcome = run_step(run, shown, note)
        self.tree = working_tree()
        if outcome.status is Status.OK and self.tree == tree and step.follows is None:
            self.keep(key, tree)
        return outcome

    def reuse(self, step: Step, key: str, tree: str, note: str) -> Outcome:
        if step.follows is None:
            self.keep(key, tree)
        return Outcome(step.name, Status.OK, note=note, reused=True)

    def keep(self, key: str, tree: str) -> None:
        # Another run may have kept passes since this one started: keep theirs.
        self.trees = Passes.load() | {key: tree}
        written = PASSES.with_name(f"passes-{os.getpid()}.tmp")
        written.write_text(json.dumps(self.trees, indent=1))
        written.replace(PASSES)


def step_key(step: Step) -> str:
    env = {name: value for name, value in os.environ.items() if STEP_ENV.match(name)}
    env |= step.env
    tools = [toolchain(), nextest()] if step.kind is Kind.TESTS else [toolchain()]
    identity = [step.name, list(step.argv), sorted(env.items()), tools]
    return hashlib.sha256(json.dumps(identity).encode()).hexdigest()


@functools.cache
def toolchain() -> str:
    return version(["rustc", "-V"])


@functools.cache
def nextest() -> str:
    return version(["cargo", "nextest", "--version"])


def version(command: list[str]) -> str:
    return subprocess.run(command, cwd=SOURCE, capture_output=True, text=True).stdout.split("\n")[0]


def refuse_arguments(name: str, args: list[str]) -> None:
    if args:
        print(f"drive.py {name} takes no arguments", file=sys.stderr)
        raise SystemExit(2)


def check(args: list[str]) -> int:
    refuse_arguments("check", args)
    # The lints first, as they answer in seconds where `index` waits for every test binary; the
    # index before the tests, as a stale one fails every test that loads its package.
    tests = replace(tests_step(), needs_previous=True)
    steps = [fmt_step(), lint_step(), ship_step(), index_step(), tests]
    return run_steps("check", steps, reuse=True)


def test(args: list[str]) -> int:
    split = args.index("--") if "--" in args else len(args)
    own, rest = args[:split], args[split:]
    env = {"CAMPFIRE_BLESS": "1"} if "--bless" in own else {}
    own = [arg for arg in own if arg != "--bless"]
    # A run of the whole suite keeps its pass for the checks; a filtered one is a step of its own.
    whole = not own and not rest
    return run_steps("test", [tests_step(own + rest, env, always_runs=True)], reuse=whole)


def index(args: list[str]) -> int:
    refuse_arguments("index", args)
    return run_steps("index", [index_step()], reuse=True)


def bench(args: list[str]) -> int:
    return run_steps("bench", [bench_step(args)])


def lan(args: list[str]) -> int:
    refuse_arguments("lan", args)
    steps = [replace(step, always_runs=True) for step in (lan_build_step(), lan_run_step())]
    return run_steps("lan", steps, reuse=True)


# Where the Zero Hour check writes its packages, each run in place of the last.
ZERO_HOUR_OUT = Path.home() / "zh-package"


def zero_hour(args: list[str]) -> int:
    # `--bless` writes the check's golden file again of what the run measured.
    env = {"CAMPFIRE_BLESS": "1"} if "--bless" in args else {}
    out = [arg for arg in args if arg != "--bless"] or [str(ZERO_HOUR_OUT)]
    argv = cargo(
        "run", "--profile", "release-dev", "--locked", "-p", "campfire-zero-hour-check", "--", *out,
    )
    step = Step("zero hour", Kind.PROGRAM, argv, env, always_runs=True)
    return run_steps("zero-hour", [step])


def package(args: list[str]) -> int:
    # The release profile ships: fat LTO, and its debug info full, which each platform then moves
    # out of the binary into a file beside it, in target/package.
    refuse_arguments("package", args)
    packages = [arg for name in SHIPPED_BINARIES for arg in ("-p", name)]
    build = Step(
        "release build", Kind.CHECK,
        cargo("build", "--release", "--locked", "--bins", *packages, JSON),
        PLATFORM.debug_split.build_env(), always_runs=True,
    )
    out = workspace().target / "package"
    steps = [build, Step("package dir", Kind.PROGRAM, (*MAKE_DIR, str(out)), needs_previous=True)]
    release = workspace().target / "release"
    for name in SHIPPED_BINARIES:
        steps += PLATFORM.debug_split.steps(name, release / f"{name}{PLATFORM.exe_suffix}", out)
    return run_steps("package", steps)


def gate(args: list[str]) -> int:
    refuse_arguments("gate", args)
    steps = []
    # Outside CI the gate writes the format, as a check does; in CI an unformatted tree fails it.
    if not IN_CI:
        steps.append(fmt_step())
    elif PLATFORM.formats_in_ci:
        steps.append(fmt_check_step())
    steps += [lint_step(), ship_step(), tests_step()]
    # After the tests, so target/debug holds the LAN check's build of each binary, which CI keeps.
    steps += [lan_build_step(), lan_run_step(), bench_check_step()]
    # CI checks Windows on Windows itself.
    if not IN_CI and PLATFORM.cross_target is not None:
        steps.append(cross_step(PLATFORM.cross_target))
    return run_steps("gate", steps, reuse=True)


COMMANDS: dict[str, tuple[Callable[[list[str]], int], str, str]] = {
    "check": (check, "", "fmt, lint, ship, index, tests: the check after a change"),
    "test": (test, "[--bless] [nextest args]", "the tests, all or filtered, each time"),
    "index": (index, "", "writes each workspace package's package.index from its files"),
    "bench": (bench, "[cargo bench args]", "the benches of every crate that has one"),
    "lan": (lan, "", "the LAN check"),
    "zero-hour": (
        zero_hour,
        "[--bless] [out]",
        "the Zero Hour check, which needs GENERALSZH_DATA; writes ~/zh-package; --bless its golden",
    ),
    "gate": (gate, "", "every step CI runs"),
    "package": (package, "", "the shipped binaries, stripped, with their debug info, in target/package"),
}


def usage() -> str:
    lines = ["usage: drive.py <command> [arguments]", ""]
    for name, (_, arguments, summary) in COMMANDS.items():
        lines.append(f"  {(name + ' ' + arguments).strip():<36} {summary}")
    return "\n".join(lines)


def stop(signum: int, frame: object) -> None:
    raise KeyboardInterrupt


def main(argv: list[str]) -> int:
    sys.stdout.reconfigure(line_buffering=True, errors="replace")
    LOGS.mkdir(parents=True, exist_ok=True)
    # A harness that ends the driver ends the step it runs too, which would hold cargo's lock.
    signal.signal(signal.SIGTERM, stop)
    if not argv or argv[0] in ("help", "-h", "--help"):
        print(usage())
        return 0 if argv else 2
    name, args = argv[0], argv[1:]
    if name not in COMMANDS:
        print(f"drive.py: no command {name!r}\n\n{usage()}", file=sys.stderr)
        return 2
    try:
        return COMMANDS[name][0](args)
    except KeyboardInterrupt:
        return 130


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
