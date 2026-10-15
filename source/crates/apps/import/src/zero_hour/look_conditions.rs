use campfire_values::DeclaredName;

/// The conditions of a map that a model's state follows, as the original sets them on every
/// object with `ForceModelsToFollowTimeOfDay` and `ForceModelsToFollowWeather`: `NIGHT` on a night
/// map and `SNOW` on a snowy one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct LookConditions {
    pub(crate) night: bool,
    pub(crate) snow: bool,
}

impl LookConditions {
    /// The conditions a look writes a variant for: a night map's, a snowy map's, and both.
    pub(crate) const VARIANTS: [LookConditions; 3] = [
        LookConditions {
            night: true,
            snow: false,
        },
        LookConditions {
            night: false,
            snow: true,
        },
        LookConditions {
            night: true,
            snow: true,
        },
    ];

    /// The names the package gives them, `night` and `snow`, in that order.
    pub(crate) fn names(self) -> Vec<DeclaredName> {
        let named = |name| DeclaredName::new(name).expect("the import's own names are declared");
        [(self.night, "night"), (self.snow, "snow")]
            .into_iter()
            .filter(|&(held, _)| held)
            .map(|(_, name)| named(name))
            .collect()
    }
}
