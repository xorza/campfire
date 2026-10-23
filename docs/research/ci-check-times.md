# CI check times

How long each platform's `check` job of CI took with the LAN check built in the dev profile and in `release-dev`, measured 2026-10-22 on the `workspace-hardening` branch.

## Method

- **Runs:** CI run 39739660428, at `e7db71c8`, built the LAN check in the dev profile; run 39744925448, at `4f7f6136`, built it with `build --profile release-dev`. Both ran on GitHub's hosted runners, from the cache master saved, which holds no `release-dev` build.
- **A time:** each `check` job's start to its end, as `gh run view --json jobs` gives them.

## Results

| Platform | Dev profile | `release-dev` |
| --- | ---: | ---: |
| Linux (`ubuntu-24.04`) | 14 min | 41 min |
| Windows (`windows-2025`) | 37 min | 79 min |
| macOS (`macos-26`) | 46 min | past 80 min |

- The jobs' limit is 90 min; the macOS job ran past 80 min.
- The difference is the build of the client and its dependencies at the release profile's optimization, with thin LTO, which no cache held.
