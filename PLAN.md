# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Effects

The slice of [Effects](docs/design/effects.md): a player of a Zero Hour skirmish sees each shot's fire effect at its barrel, each projectile's exhaust, each hit's burst and each death's explosion, as the game's FX lists and particle systems draw them. Its facts are [Zero Hour's effects](docs/research/zero-hour-effects.md); its checks are that document's.

### End of the slice

- [ ] A player sees Tournament Desert's shots, hits, trails and deaths in the client as the game draws them ([question](PLAN_QUESTIONS.md#effects-end-of-the-slice)); then the slice's decisions fold into the documents of their systems, `02-engine-core.md` for the format and the client's particles and `09-zero-hour.md` for what the import converts and loses, the code's citations of `effects.md` with them, and `effects.md` goes.

## Issue fixes

Fixes of the issue log's Ready items, found 2026-10-26; each step is one batch, as each changes one system.

### I1. A wake sends one byte until a wait takes it

Touches the socket's IO thread wake alone; it stands first, as macOS CI failed on it (run 40353599661).

- [ ] A macOS loopback datagram reaches its socket after its `send` returns, so a wake byte still in flight outlived a wait's drain and ended the next wait for nothing: a probe of the wake test's steps on an M5 failed 4 rounds of 20000 at rest and about 1 in 19 under CPU load, and none of 160000 on Linux. Commit `ff9e1346` sends a byte only while a flag says none is pending, as libuv's `uv_async_send` and `polling`'s `Poller::notify` coalesce wakes, and the same probe with the flag failed none of 180000 rounds on the M5. What remains: `Wake::take` clears the flag by a plain release store, where the clear must acquire what a wake that sent no byte brought for the pass after the wait, as libuv's `uv__async_io` clears `pending` by a read-modify-write; the decision is in no design document, and the probe's facts in no research. The clear becomes an acquiring swap, the decision goes into `11-net-stack/udp.md`, and the facts into `docs/research/udp.md`. Direction: what can be forgotten is a type. Checks: `udp_port`'s wake test passes on macOS under CPU load and in CI.

### I2. A turret carries the slots its data names

Touches the combat capability's turret and the import's turrets; it stands after I1.

- [ ] A turret carries every weapon of its slot kind, where Zero Hour's carries only the slots its `ControlledWeaponSlots` names, and aims the slots of an `AltTurret` with a second turret (`AIUpdateInterface::getWhichTurretForWeaponSlot`); the import reads only an AI module's first `Turret`. A turret carries the weapon slots its data names, and the import writes them from `ControlledWeaponSlots` and an `AltTurret`'s. No converted weapon of the import of 2026-10-26 stands in a slot its turrets do not control; the four `AmericaVehicleBattleShip` types fire one weapon from `PRIMARY` and `SECONDARY`, which their `AltTurret` aims. Direction: a capability serves every game. Checks: the combat tests aim a turret's weapon and fire a weapon of another slot at the body's facing; the import's tests write a turret's slots and an `AltTurret`'s.

## Looks of the report of 2026-10-26

Three faults a player's report showed (2026-10-26, report `1e38d2c4…-945` and its session's log to tick 5711, on Tournament Desert): a Humvee drew its gun's muzzle flash at all times; a Tomahawk kept its missile on its rail after each launch, where Zero Hour's comes back after its reload; and China's Gattling tank, attacking a Crusader from tick 5505, did not spin its barrels. Each cause is found; the steps stand in the order of their cost.

### L1. A clip of one shot is a clip

Touches the import's weapons; it stands first, as the rounds of the last batch wait for it.

- [ ] A weapon of `ClipSize = 1` converts to a period of its reload and no clip (09-zero-hour.md, A clip of one shot is a period), exact for its timing, where Zero Hour holds such a weapon in `RELOADING_CLIP` after each shot, which its draw reads: the Tomahawk's `TomahawkMissileWeapon` "uses a clip of one to get the Reloading modelcondition", and its model shows its missile from the clip's remaining ammo (`doHideShowProjectileObjects`); so the client sees no clip, draws no reload and never hides the missile on the rail. The import keeps a clip of one shot as a clip of its reload, its period the same, and the decision goes. Direction: the importer takes a game's data as it is. Checks: the import's tests convert a clip of one shot to a clip; the Zero Hour check's pins hold, its fights alike; the client's tests hide a round of a clip of one while it reloads.

### L2. A muzzle flash shows only at its shot

Touches the import's looks and the client's models.

- [ ] A state's default hides each weapon slot's muzzle flashes before its own hides and shows (`ModelNodes::hidden`), so a `ShowSubObject` of a turret shows the flash on the turret's child pivot: the Humvee's state `NONE` shows `Turret`, and with it `avhummer.muzzlefx01`, at all times. Zero Hour hides each flash every frame after the state's hides and shows, and shows it while its barrel's recoil starts (`W3DModelDraw::handleClientRecoil`). The import hides the flashes after the state's hides and shows, and keeps them out of an upgrade's shows and a clip's rounds; the client shows a barrel's flash while its recoil starts, for the frames the draw module's recoil data give. Direction: the importer takes a game's data as it is, and the client derives what the sim does not send. Checks: the import's tests keep a flash hidden under a shown turret; the client's tests show a flash at a shot and hide it after.

### L3. An attacking unit draws its attacking state

Touches the client files' tests, the import's look flags and the client's look states.

- [ ] `ATTACKING` is a condition of no test, `never`, so no unit draws a state of it: the Gattling tank's `ATTACKING` states loop its barrels' spin, and other looks name it beside their attack phases. Zero Hour sets it while its unit's AI attacks, from its attack state's start to its end, its approach and its pursuit among it (`AIAttackState::onEnter`, `onExit`). A condition test holds while its unit has an attack target, and the import gives `ATTACKING` that test. Direction: a look is states of conditions, and the client derives what the sim does not send. Checks: the client files' tests read the test; the import's tests give `ATTACKING` it; the client's tests draw an attacking state while a unit has an attack target and its default after.

## Foundations
