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
- Levels 1–18. Basic abilities have 5 ranks; the ultimate has 3, learnable at levels 6, 11 and 16.
- Resistance `r` scales damage by `100 / (100 + r)`, or by `2 − 100 / (100 − r)` when `r` is negative. Crits deal 200%. Cooldown reduction caps at 40%. Flat and percent penetration, life steal and spell vamp.
- Attacks have a wind-up, when moving cancels the attack, and a back-swing, when moving is free.
- Distances are in meters: melee range about 1.25, ranged attacks 5.5–6.5, hero move speed 3.0–3.5 per second.

## Map and items

- 3v3 on two lanes with jungle camps between them and one neutral objective.
- Some items have actives. Consumables: health and mana potions, sight and vision wards, and an elixir that grants true sight until death.

## What the game needs from the engine

It declares `combat`, `stats`, `abilities`, `projectiles`, `areas`, `orders`, `navigation` and `vision` ([Genres](04-capabilities/genres.md#target-games)), and needs from them: mana and energy; toggles with a cost per attack or per second; charges; on-hit effects and crits; marks that detonate; burns; stealth, true sight and area reveal; dashes, pulls, knock-ups and knockbacks; untargetable; channels; line, homing, bouncing and jumping attacks; delayed and persistent areas; global projectiles with effects by distance flown; auras; shields; cooldown changes on takedowns and hits; stun, slow, root, knock-up, knockback and slow immunity; charged casts with recast. The API that provides them: [Script API](08-script-api.md).
