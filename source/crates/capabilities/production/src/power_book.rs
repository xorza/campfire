use bevy_ecs::resource::Resource;
use campfire_declarations::{Book, BookCx, BookError, BookPlace, Section};
use campfire_names::{MatchNames, UnknownName};
use campfire_stats::{ModifierAppliers, ModifierBook, StatBook, Way};
use campfire_units::StatId;
use campfire_values::{ModifierId, Named, PackageIndex};

use crate::mode_power_data::ModePowerData;
use crate::power_speed::PowerSpeed;

/// The mode's `[power]`, its names resolved; none in a mode that counts no power. Package data,
/// not state (power.md, Power is production's).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
#[component(immutable)]
pub struct PowerBook(pub(crate) Option<PowerRules>);

/// What the mode counts power by: the stat each unit gives or takes, the modifier a short player
/// holds, none for none, and the speed of a short player's production, none for none slowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PowerRules {
    pub(crate) stat: StatId,
    pub(crate) short: Option<ModifierId>,
    pub(crate) speed: Option<PowerSpeed>,
}

impl PowerBook {
    /// Whether the mode counts power.
    pub(crate) const fn counts(self) -> bool {
        self.0.is_some()
    }

    /// The stat each unit gives or takes power by, none in a mode that counts no power.
    pub(crate) fn stat(self) -> Option<StatId> {
        self.0.map(|rules| rules.stat)
    }

    /// The speed of a short player's production, none in a mode that slows none.
    pub(crate) fn speed(self) -> Option<PowerSpeed> {
        self.0?.speed
    }

    /// The mode's `[power]`, its stat and its modifier resolved; an error for a name the mode
    /// does not declare, and for a speed that breaks its rules.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let Some(data) = cx.mode_section::<ModePowerData>() else {
            return Ok(Self(None));
        };
        let at = || BookPlace::Section(ModePowerData::KEY);
        let stats = cx.book::<StatBook>()?;
        let stat = stats.named_in(PackageIndex::MODE, at(), &data.stat)?;
        let short = match &data.short {
            None => None,
            Some(name) => {
                let modifiers = cx.book::<ModifierBook>()?;
                let named = modifiers.names().named((PackageIndex::MODE, name.as_str()));
                Some(named.ok_or_else(|| {
                    let name = UnknownName::new(ModifierId::NAMES, name);
                    BookError::unknown(PackageIndex::MODE, at(), name)
                })?)
            }
        };
        let speed = data.speed.as_ref().map(PowerSpeed::new).transpose();
        let speed = speed.map_err(|problem| BookError::rule(PackageIndex::MODE, at(), problem))?;
        Ok(Self(Some(PowerRules { stat, short, speed })))
    }

    /// Adds the modifier a short player holds as applied by no action, as its player holds it,
    /// so the load checks it reads no param that way does not give.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn applied_ways(
        cx: &BookCx<'_>,
        appliers: &mut ModifierAppliers,
    ) -> Result<(), BookError> {
        let short = cx
            .mode_section::<ModePowerData>()
            .and_then(|data| data.short.as_ref());
        if let Some(short) = short {
            appliers.apply(PackageIndex::MODE, short.as_str(), Way::NoAction);
        }
        Ok(())
    }
}

impl Book for PowerBook {
    const SCRIPTS_ONLY: bool = false;

    /// The production column takes it from the match at each read.
    fn share(&self, _: &mut MatchNames) {}
}
