# Campfire — Reference MOBA

Draft. Names are working names. Numbers come later, with balance.

## Heroes

| Hero | Role | Resource |
| --- | --- | --- |
| Rime | Ranged marksman | Mana |
| Veil | Melee assassin | Energy |
| Kensho | Melee attack carry | Mana |
| Cinder | Burst mage | Mana |
| Husk | Tank | Mana |
| Gale | Support | Mana |

**Rime**
- *Stillness* (passive): crit chance rises while Rime does not attack; an attack resets it.
- *Chill Arrows* (toggle): each attack costs mana and slows the target.
- *Fan of Frost*: a cone of arrows that damage and slow.
- *Snow Owl*: passive bonus gold per kill; active sends an owl along a line that reveals the area it passes.
- *Glacier Arrow* (ultimate): a global line projectile; the first enemy hero hit is stunned longer the farther the arrow flew, and enemies around it take damage and are slowed.

**Veil**
- *Dual Path* (passive): attacks deal bonus magic damage; ability damage heals Veil.
- *Dusk Mark*: a targeted projectile marks the enemy; Veil's next attack or ability on it detonates the mark for damage and restores energy.
- *Smoke Ring*: an area that makes Veil stealthed and tougher while inside; enemies inside are slowed.
- *Whirling Blades*: damage around Veil; detonates marks.
- *Night Step* (ultimate): dash to an enemy; uses charges that refill over time and on takedowns.

**Kensho**
- *Twin Cut* (passive): every Nth consecutive attack hits twice.
- *Flicker Strike*: Kensho becomes untargetable and strikes the target and up to three enemies near it, with a chance of bonus damage to minions.
- *Still Mind*: channel that heals and reduces damage taken.
- *Honed Edge*: passive bonus attack damage; the active raises it for a while, then the passive is lost until the cooldown ends.
- *Unbound* (ultimate): bonus move and attack speed and immunity to slows; takedowns shorten Kensho's cooldowns.

**Cinder**
- *Kindle* (passive): abilities set the target ablaze, a burn over a few seconds.
- *Fire Lance*: a line skillshot; stuns a target that is ablaze.
- *Eruption*: a ground area that erupts after a delay; more damage to targets ablaze.
- *Wildfire*: a targeted nuke; if the target is ablaze, it spreads to enemies nearby.
- *Chain Fire* (ultimate): a fireball that bounces between nearby enemies up to five times.

**Husk**
- *Withering Touch* (passive): attacks lower the target's magic resistance.
- *Grasping Wraps*: a line skillshot; Husk pulls himself to the enemy hit and stuns it.
- *Dread* (toggle): an aura that deals a share of each nearby enemy's max health per second, for mana per second.
- *Lash Out*: passive flat damage reduction; the active damages enemies around Husk, and each hit Husk takes shortens its cooldown.
- *Tomb Bind* (ultimate): enemies around Husk cannot move or attack for a short time.

**Gale**
- *Fair Wind* (passive): nearby allies move faster.
- *Cyclone*: place a tornado that grows while it charges, then travels in a line and knocks enemies up; recast releases it early.
- *Gust*: passive bonus move speed; the active damages and slows a target.
- *Wind Shield*: shields an ally, who gains attack damage while shielded.
- *Tempest* (ultimate): knocks enemies back around Gale, then channels a heal on allies around her.

## Player spells

Each player picks two: Blink (short teleport), Haste (move speed), Mend (heal self and allies near), Scorch (burn that reduces healing), Sap (slow and weaken an enemy), Farsight (reveal an area anywhere). Each is an ordinary scripted ability.

## Rules

- Crit and every other chance effect roll independently from the secret RNG stream.
- Vision: brush blocks sight from outside it; sight wards and vision wards; stealth, revealed by true sight (vision wards, towers and consumables).
- Last-hitting gives gold; there are no denies.
- **Kill gold.** A hero kill pays its killer the kill gold, and the first of the match pays first-blood gold more; the assisters split the assist gold evenly. A hero that kills from its second kill on without dying carries a **streak bounty**, a set amount for each kill of the streak up to a cap, which its killer takes with the kill gold; its death ends the streak. This is League of Legends' classic shutdown: one number a player reads on the scoreboard, and a way back for the team that is behind. The amounts are the mode's params.
- **Camp monsters** stand at their spawn place, within `home_slack`, and attack a unit they see that struck them in the last `aggro_window_ms`. One drawn past its `leash_range` resets: it walks home, ignoring every attacker, and arrives with full health ([AI](04-capabilities/control.md#ai)).
- **Camps** come back whole, as League of Legends' do: a camp's timer starts when its last monster dies, and then every monster of the camp spawns again at its place. A camp is a map marker whose params list its monsters and its respawn time; a camp never stands half full.
- Levels 1–18, a point each. Basic abilities have 5 ranks, learnable at levels 1, 3, 5, 7 and 9, as in League of Legends; the ultimate has 3, learnable at levels 6, 11 and 16. A player learns a rank with Ctrl and the ability's key.
- Resistance `r` scales damage by `100 / (100 + r)`, or by `2 − 100 / (100 − r)` when `r` is negative. Crits deal 200%. Cooldown reduction caps at 40%. Flat and percent penetration, life steal and spell vamp.
- Attacks have a wind-up, when moving cancels the attack, and a back-swing, when moving is free.
- **Creeps** walk their lane and, within their aggro range, attack the first of: an enemy hero that struck an allied hero near them; the target they have, while it lives and stays in reach; the nearest enemy creep; the nearest enemy hero; the nearest enemy structure. With none, they walk on. Units go before buildings, as League of Legends' minions and Dota 2's lane creeps choose, and a creep keeps a structure it attacks until a hero needs defending, as a minion ignores calls for help while it attacks a turret. So a wave that wins its lane takes the towers, the inhibitor and the core in its way.
- **Towers** attack, within their range, the first of: an enemy hero that struck an allied hero there; the target they have, while it stays in range; the nearest enemy creep; the nearest enemy hero.
- Distances are in meters: melee range about 1.25, ranged attacks 5.5–6.5, hero move speed 3.0–3.5 per second.

## Map and items

- 3v3 on two lanes with jungle camps between them and one neutral objective.
- Some items have actives. Consumables: health and mana potions, sight and vision wards, and an elixir that grants true sight until death. Items build from components, and players buy them in their team's base, or anywhere while dead, as in League of Legends ([Items](04-capabilities/items.md)).
- A sight ward is a unit the ward's action spawns where it aims: hidden, with sight, a life of a few hits, gone after its time; a vision ward is one with true sight that its team sees, and no time limit.

## What the game needs from the engine

It declares `combat`, `stats`, `abilities`, `projectiles`, `areas`, `orders`, `navigation`, `vision`, `progression` and `items` ([Genres](04-capabilities/genres.md#target-games)). In the engine's model ([The model](04-capabilities/00-overview.md#the-model)) it is two teams and a hostile team of camps, a planar map with one layer, the pools health, mana and energy, the slot kinds `basic`, `ultimate`, `spell` and `weapon`, the choices `hero` and `spells`, and the tags of crowd control, stealth and true sight that design 08 lists; crowd control is its own modifiers, applied with `ctx.add_modifier`. It needs from the engine: mana and energy; toggles with a cost per attack or per second; charges; on-hit effects and crits; marks that detonate; burns; stealth, true sight and area reveal; dashes, pulls, knock-ups and knockbacks; untargetable; channels; line, homing, bouncing and jumping attacks; delayed and persistent areas; global projectiles with effects by distance flown; auras; shields; cooldown changes on takedowns and hits; stun, slow, root, knock-up, knockback and slow immunity; charged casts with recast. The API that provides them: [Script API](08-script-api.md).
