#!/usr/bin/env python3
"""Campfire's driver: the one way to check, test, document and bench the workspace.

Cargo keeps a cache for each shape of a command, its features, profile, mode and flags, so the
driver owns every invocation (docs/design/02-engine-core.md, The driver). `./drive.py help` lists
the commands.
"""

from __future__ import annotations

import enum
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path

SOURCE = Path(__file__).resolve().parent
OWN = SOURCE / "target" / "drive"
LOGS = OWN / "log"
# One JSON line for each step a command ran, so the checks' times are measured, not assumed.
HISTORY = OWN / "history.jsonl"
# For each step of `check`, the tree it last passed on.
PASSES = OWN / "passes.json"
# A copy of git's index, which the tree of the working files is written through.
TREE_INDEX = OWN / "tree-index"
LAN_ROOT = OWN / "lan-check"

IN_CI = "GITHUB_ACTIONS" in os.environ
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
    "--show-progress", "none", "--status-level", "fail",
    "--final-status-level", "slow", "--failure-output", "final", "--success-output", "never",
)
# The one test that compares each workspace package to its `package.index`, and writes the index
# again from the package's files when the run blesses: the format stays with `package` alone.
INDEX_TEST = (
    "package(campfire-package) & "
    "test(=package_dir::tests::every_workspace_package_holds_the_index_of_its_files)"
)
# The crates whose tests read the files of every crate, the workspace's rules: a selection of
# tests always holds them, as a change anywhere is their input. A new test that reads other crates'
# files goes in one of them.
WORKSPACE_READERS = ("campfire-common", "campfire-log")
LAN_PACKAGES = (
    "campfire-lan-check", "campfire-server", "campfire-client", "campfire-verifier",
)

PROGRESS = re.compile(
    r"^\s*(Compiling|Checking|Documenting|Finished|Fresh|Blocking|Downloading|Downloaded|"
    r"Updating|Locking|Adding|Building|Generated|Running|Scraping)\s"
)
# Nextest's run header and its notices of a test still running: its summary lists each slow test.
NEXTEST_PROGRESS = re.compile(r"^\s*(Nextest run ID .*|Starting \d+ tests? .*|\[>\s*[\d.]+s\] .*|─+)$")
# A unit cargo had to build.
BUILT = re.compile(r"^\s*(Compiling|Checking|Documenting)\s")
# What changes a step's result from outside the working files: the toolchain's settings, cargo's,
# and the tests' own.
STEP_ENV = re.compile(r"^(RUST\w*|CARGO\w*|CAMPFIRE_\w+)$")
ANSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")


class Platform:
    """What differs between the OSes the driver runs on: no caller tests the OS."""

    exe_suffix = ""
    cross_target: str | None = None
    # Formatting is the same on every OS, so CI checks it on one.
    formats_in_ci = False


class Linux(Platform):
    cross_target = "x86_64-pc-windows-gnu"
    formats_in_ci = True


class MacOs(Platform):
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
    # A nextest run that may run only the tests of the crates a change reaches.
    selects: bool = False

    def selecting(self, packages: list[str]) -> Step:
        reached = [f"rdeps(={package})" for package in packages]
        expression = " | ".join([*reached, *(f"package(={crate})" for crate in WORKSPACE_READERS)])
        argv = [*self.argv, "-E", expression, "--no-tests=pass"]
        return Step(self.name, argv, self.env, self.needs_previous)

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
    built: int
    # It passed on this tree before, so it did not run.
    reused: bool = False

    def line(self) -> str:
        text = f"{self.name:<12} {self.status.value:<4} {self.seconds:6.1f} s"
        if self.status is Status.FAIL and self.log is not None:
            text += f"  {shown_path(self.log)}"
        if self.reused:
            text += "  passed on this tree"
        return text


def cargo(*args: str) -> list[str]:
    return ["cargo", *args]


def nextest_output() -> tuple[str, ...]:
    return ("--profile", "ci") if IN_CI else QUIET_NEXTEST


def index_step() -> Step:
    return Step(
        "index", cargo(*TESTS, *nextest_output(), "-E", INDEX_TEST), {"CAMPFIRE_BLESS": "1"}
    )


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


def tests_step(
    extra: list[str] | None = None, env: dict[str, str] | None = None, selects: bool = False,
) -> Step:
    argv = cargo(*TESTS, *nextest_output(), *(extra or []))
    return Step("tests", argv, env or {}, selects=selects)


def bench_cases_step() -> Step:
    return Step("bench cases", cargo(*TESTS, *nextest_output(), "--bench", "*"))


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
        if PROGRESS.match(line) or NEXTEST_PROGRESS.match(line):
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
    printed = hidden = built = 0
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
                    if BUILT.match(line):
                        built += 1
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
    return Outcome(step.name, status, time.monotonic() - start, log, printed, built)


def run_steps(command: str, steps: list[Step], passes: Passes | None = None) -> int:
    outcomes: list[Outcome] = []
    for step in steps:
        if step.needs_previous and outcomes and outcomes[-1].status is not Status.OK:
            outcome = Outcome(step.name, Status.SKIP, 0.0, None, 0, 0)
        elif passes is None:
            outcome = run_step(step)
        else:
            outcome = passes.run(step)
        print(outcome.line())
        record(command, outcome)
        outcomes.append(outcome)
    # Diagnostics between the status lines leave them apart: gather them again at the end.
    if len(outcomes) > 1 and any(outcome.printed for outcome in outcomes):
        print()
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
    }
    OWN.mkdir(parents=True, exist_ok=True)
    with HISTORY.open("a", encoding="utf-8") as history:
        history.write(json.dumps(entry) + "\n")


def git(*args: str, env: dict[str, str] | None = None) -> str | None:
    found = subprocess.run(
        ["git", *args], cwd=SOURCE, env=os.environ | (env or {}), capture_output=True, text=True,
    )
    return found.stdout if found.returncode == 0 else None


def working_tree() -> str | None:
    """The id of git's tree of the working files as they stand, tracked and new alike, as
    `.gitignore` leaves them; None outside git. It is written through a copy of the index, so the
    index itself stays as it is."""
    index = git("rev-parse", "--path-format=absolute", "--git-path", "index")
    if index is None:
        return None
    OWN.mkdir(parents=True, exist_ok=True)
    try:
        shutil.copyfile(index.strip(), TREE_INDEX)
    except FileNotFoundError:
        TREE_INDEX.unlink(missing_ok=True)
    env = {"GIT_INDEX_FILE": str(TREE_INDEX)}
    if git("add", "--all", "--", ":/", env=env) is None:
        return None
    tree = git("write-tree", env=env)
    return tree.strip() if tree else None


class Passes:
    """The tree each step of `check` last passed on. A step does not run again on that tree, and
    the tests run only in the crates that the change since it reaches, through nextest's
    `rdeps()`, which follows every dependency, dev-dependencies too: the tests outside them
    passed on that tree, and nothing they build changed; the tests of `WORKSPACE_READERS` always
    run. A change to a manifest, or outside every crate, as to the lock, the lint or test
    settings, a package of game data or a document a test reads, runs every test."""

    def __init__(self) -> None:
        try:
            self.trees: dict[str, str] = json.loads(PASSES.read_text())
        except (FileNotFoundError, json.JSONDecodeError):
            self.trees = {}
        rustc = subprocess.run(["rustc", "-V"], cwd=SOURCE, capture_output=True, text=True)
        self.toolchain = rustc.stdout.strip()

    def key(self, step: Step) -> str:
        env = {name: value for name, value in os.environ.items() if STEP_ENV.match(name)}
        identity = [step.name, step.argv, sorted((env | step.env).items()), self.toolchain]
        return hashlib.sha256(json.dumps(identity).encode()).hexdigest()

    def run(self, step: Step) -> Outcome:
        tree = working_tree()
        key = self.key(step)
        last = self.trees.get(key)
        if tree is not None and tree == last:
            return Outcome(step.name, Status.OK, 0.0, None, 0, 0, reused=True)
        run = step
        if step.selects and tree is not None and last is not None:
            packages = reached_packages(last, tree)
            if packages is not None:
                print(f"tests of the crates that depend on {', '.join(packages)}, and the rules")
                run = step.selecting(packages)
        outcome = run_step(run)
        # A file changed while it ran leaves the tree unchecked as a whole.
        if outcome.status is Status.OK and tree is not None and working_tree() == tree:
            self.trees[key] = tree
            OWN.mkdir(parents=True, exist_ok=True)
            written = PASSES.with_suffix(".tmp")
            written.write_text(json.dumps(self.trees, indent=1))
            written.replace(PASSES)
        return outcome


def reached_packages(since: str, tree: str) -> list[str] | None:
    """The workspace's crates that hold a file changed from tree `since` to `tree`; None when a
    changed file is in no crate or is a manifest, whose features unify into crates that do not
    depend on it, or when `since` is gone."""
    changed = git("diff", "--name-only", "-z", since, tree)
    top = git("rev-parse", "--show-toplevel")
    if changed is None or top is None:
        return None
    folders = [
        (Path(package["manifest_path"]).parent, package["name"])
        for package in metadata()["packages"]
    ]
    found = set()
    for name in filter(None, changed.split("\0")):
        path = Path(top.strip()) / name
        if path.name == "Cargo.toml":
            return None
        owners = [(folder, package) for folder, package in folders if path.is_relative_to(folder)]
        if not owners:
            return None
        found.add(max(owners, key=lambda owner: len(owner[0].parts))[1])
    return sorted(found)


def refuse_arguments(name: str, args: list[str]) -> None:
    if args:
        print(f"drive.py {name} takes no arguments", file=sys.stderr)
        raise SystemExit(2)


def check(args: list[str]) -> int:
    refuse_arguments("check", args)
    # The lints first, as they answer in seconds where `index` waits for every test binary; the
    # index before the tests, as a stale one fails every test that loads its package.
    steps = [fmt_step(), lint_step(), ship_step(), index_step(), tests_step(selects=True)]
    return run_steps("check", steps, Passes())


def test(args: list[str]) -> int:
    split = args.index("--") if "--" in args else len(args)
    own, rest = args[:split], args[split:]
    env = {"CAMPFIRE_BLESS": "1"} if "--bless" in own else {}
    own = [arg for arg in own if arg != "--bless"]
    return run_steps("test", [tests_step(own + rest, env)])


def index(args: list[str]) -> int:
    refuse_arguments("index", args)
    return run_steps("index", [index_step()])


def doc(args: list[str]) -> int:
    refuse_arguments("doc", args)
    return run_steps("doc", [rustdoc_step()])


def bench(args: list[str]) -> int:
    return run_steps("bench", [bench_step(args)])


def lan(args: list[str]) -> int:
    refuse_arguments("lan", args)
    return run_steps("lan", [lan_build_step(), lan_run_step()])


def zero_hour(args: list[str]) -> int:
    return run_steps("zero-hour", [
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
        lint_step(), ship_step(), tests_step(), bench_cases_step(), rustdoc_step(),
    ]
    # CI checks Windows on Windows itself.
    if not IN_CI and PLATFORM.cross_target is not None:
        steps.append(cross_step(PLATFORM.cross_target))
    # Last, so target/debug holds the LAN check's build of each binary, which CI keeps.
    steps += [lan_build_step(), lan_run_step()]
    return run_steps("gate", steps)


COMMANDS: dict[str, tuple[Callable[[list[str]], int], str, str]] = {
    "check": (check, "", "fmt, lint, ship, index, tests: the check after a change"),
    "test": (test, "[--bless] [nextest args]", "the tests, all or filtered"),
    "index": (index, "", "writes each workspace package's package.index from its files"),
    "doc": (doc, "", "rustdoc with -D warnings"),
    "bench": (bench, "[cargo bench args]", "the benches of every crate that has one"),
    "lan": (lan, "", "the LAN check"),
    "zero-hour": (zero_hour, "[run root]", "the Zero Hour check, which needs GENERALSZH_DATA"),
    "gate": (gate, "", "every step CI runs"),
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
    if name not in COMMANDS:
        print(f"drive.py: no command {name!r}\n\n{usage()}", file=sys.stderr)
        return 2
    try:
        return COMMANDS[name][0](args)
    except KeyboardInterrupt:
        return 130


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
