# Spectating and the host's visibility settings

What [Replication audiences](../design/14-replication-audiences.md)' host and spectator settings are taken from: how established games split what spectators and players out of play see between the game and its host. Facts only, each with its source; what a source left open says so.

## Counter-Strike

- `mp_forcecamera` sets what a dead player may watch: `0` any team, `1` its own team only, `2` no one, a black screen until the next round. A third-party list gives `1` as its default; Valve's own pages found give no values. ([TotalCS: mp_forcecamera](https://totalcsgo.com/commands/mpforcecamera))
- A game mode's defaults sit in its `gamemode_<mode>.cfg`, and a server's operator overrides them in `gamemode_<mode>_server.cfg`, which runs after it. Valve's wiki names the `_server` files as those of a server's multiplayer convars; the load order comes from hosting guides, not from Valve. ([Valve: Game mode](https://developer.valvesoftware.com/wiki/Game_mode), [cm2network/cs2](https://hub.docker.com/r/cm2network/cs2), [xGamingServer: config files](https://xgamingserver.com/docs/counter-strike-2/config-files))
- A dead player who tells its living team what it saw is called ghosting; `mp_forcecamera 1` keeps it to what its team saw.

## Dota 2

- A lobby's host turns spectators on or off, off by default, and sets the DotaTV delay: 10 s, 2 min, 5 min or 15 min, 2 min by default. ([Liquipedia: Lobby](https://liquipedia.net/dota2/Lobby))
- A spectator chooses its own fog: one team's vision, or both teams' together. No source found says the host can set it. ([Liquipedia: Spectating](https://liquipedia.net/dota2/Spectating))

## League of Legends

- The spectator delay is 180 s on Summoner's Rift and 60 s on Howling Abyss, there so a spectator cannot tell a player what it sees; a custom game holds at most 4 spectators. ([League of Legends Wiki: Spectator Mode](https://wiki.leagueoflegends.com/en-us/Spectator))
- A tournament lobby's spectators are none, the lobby's alone, or all. ([start.gg: League of Legends tournament codes](https://help.start.gg/article/league-of-legends-tournament-codes-on-startgg))

## Unreal Engine

- `AGameModeBase::CanSpectate` decides whether a viewer may watch from a view target, the hook a game overrides to keep a spectator on its team. ([Epic: AGameModeBase](https://dev.epicgames.com/documentation/en-us/unreal-engine/API/Runtime/Engine/AGameModeBase))
- `UGameSessionSettings::MaxSpectators` caps a server's spectators, a `GlobalConfig` property of a `Config=Game` class: a value the game gives, which a server's ini sets again. ([Epic: UGameSessionSettings](https://dev.epicgames.com/documentation/unreal-engine/API/Runtime/EngineSettings/UGameSessionSettings), [Epic: AGameSession](https://dev.epicgames.com/documentation/unreal-engine/API/Runtime/Engine/AGameSession))

## Open

- What a defeated player of an RTS sees: no source found for StarCraft II or Age of Empires II, and Zero Hour's rule waits for GeneralsX, which is not read yet.
