use campfire_common::{PlayerSlot, Tick};
use campfire_sim::StableId;
use campfire_values::{
    ActionId, ActionStart, Allegiance, DashDelivery, Hit, ModifierId, PackageIndex, PlayerView,
    Rank,
};

use crate::scripts::mode_scripts::ListPlace;
use crate::scripts::script_role::ScriptRole;

/// What a call starts with: its role; whether it writes or only reads; its acting unit, a cast's
/// caster, a modifier's source or the unit that thinks, none for the mode, and a cast's caster's
/// allegiance as its action began, which outlives the caster; the action whose params
/// it reads, at `rank`, and its modifier's; the package whose names it means, 0 the mode's; the
/// depth of the chain of combat events it runs in, 0 outside one; the hit a delivery's hook runs
/// for, which the damage it deals carries; how its action started; the delivery a dash it starts
/// carries, an instant action's own in its `on_resolve`; and the player whose budget pays for it,
/// the acting unit's controller or the player whose input it answers, none when no player does;
/// for a call of the mode's role, the place of its script in the mode's list; and the player whose
/// view a bot's call reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallStart {
    pub role: ScriptRole,
    pub purity: Purity,
    pub acting: Option<StableId>,
    pub allegiance: Option<Allegiance>,
    pub action: Option<ActionId>,
    pub rank: Rank,
    pub modifier: Option<ModifierId>,
    pub package: PackageIndex,
    pub depth: u8,
    pub hit: Option<Hit>,
    pub start: Option<ActionStart>,
    pub dash_delivers: Option<DashDelivery>,
    pub payer: Option<PlayerSlot>,
    /// Only the mode's own script holds the mode's state and choices, and ends the match.
    pub listed: Option<ListPlace>,
    /// The tick the call runs in, which its effects apply in.
    pub now: Tick,
    /// The player whose view the call reads, a bot's player slot's; none for a call of the sim's
    /// roles, which read the match whole.
    pub sight: Option<PlayerView>,
}

/// A cast of `action` of `package` at `rank` by `caster`, of `allegiance` as it began.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionCast {
    pub action: ActionId,
    pub rank: Rank,
    pub caster: StableId,
    pub allegiance: Allegiance,
    pub package: PackageIndex,
}

/// What a call does when it finds its budget spent: it fails, as a call a player causes does, or
/// it waits for a later tick, as the engine's own calls do (02-engine-core.md, Operation budgets).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spent {
    Fails,
    Waits,
}

/// What a call may do with the match: queue effects and write state, or, as a pure hook's,
/// only read. A pure call draws from no budget, and applies nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purity {
    Writes,
    Reads,
}

impl CallStart {
    /// A call of `role` in tick `now`, which writes, with no acting unit, action, modifier or
    /// chain, of the mode's package, at rank 1.
    pub const fn mode(role: ScriptRole, now: Tick) -> Self {
        Self {
            role,
            purity: Purity::Writes,
            acting: None,
            allegiance: None,
            action: None,
            rank: Rank::FIRST,
            modifier: None,
            package: PackageIndex::MODE,
            depth: 0,
            hit: None,
            start: None,
            dash_delivers: None,
            payer: None,
            listed: None,
            now,
            sight: None,
        }
    }

    /// What the call does when it finds its budget spent: an AI's think, or a mode's event, which
    /// no player causes, waits; an action's, a modifier's and a mode input's call fails.
    pub const fn when_spent(&self) -> Spent {
        match (self.role, self.payer) {
            (ScriptRole::Ai | ScriptRole::Bot, _) | (ScriptRole::Mode, None) => Spent::Waits,
            (ScriptRole::Mode, Some(_)) | (ScriptRole::Action | ScriptRole::Modifier, _) => {
                Spent::Fails
            }
        }
    }

    /// A call of the mode's script at `place` in its list, in tick `now`.
    pub const fn listed(place: ListPlace, now: Tick) -> Self {
        Self {
            listed: Some(place),
            ..Self::mode(ScriptRole::Mode, now)
        }
    }

    /// The pure hook of the mode's script at `place` in its list, in tick `now`, whose `ctx` only
    /// reads.
    pub const fn pure(place: ListPlace, now: Tick) -> Self {
        Self {
            purity: Purity::Reads,
            ..Self::listed(place, now)
        }
    }

    /// The think before tick `now` of the bot that plays as `player` views the match, outside the
    /// sim: a call that reads, whose payer is the bot's player slot, as it orders the player slot's
    /// units, and that sees what the player slot's client receives (03-game-scripting.md, A bot
    /// decides from what it sees, each think).
    pub const fn bot(player: PlayerView, now: Tick) -> Self {
        Self {
            purity: Purity::Reads,
            payer: Some(player.player_slot),
            sight: Some(player),
            ..Self::mode(ScriptRole::Bot, now)
        }
    }

    /// `on_think` of `id` in tick `now`.
    pub const fn think(id: StableId, now: Tick) -> Self {
        Self {
            acting: Some(id),
            ..Self::mode(ScriptRole::Ai, now)
        }
    }

    /// A cast `cast` in tick `now`, for no hit.
    pub const fn cast(cast: ActionCast, now: Tick) -> Self {
        Self {
            acting: Some(cast.caster),
            allegiance: Some(cast.allegiance),
            action: Some(cast.action),
            rank: cast.rank,
            package: cast.package,
            ..Self::mode(ScriptRole::Action, now)
        }
    }

    /// A hook of `modifier` of `package` at chain depth `depth` in tick `now`, of an instance
    /// from no source, by no action, at rank 1.
    pub const fn hook(modifier: ModifierId, package: PackageIndex, depth: u8, now: Tick) -> Self {
        Self {
            modifier: Some(modifier),
            package,
            depth,
            ..Self::mode(ScriptRole::Modifier, now)
        }
    }
}
