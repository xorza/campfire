use campfire_common::NamedEnum;

use crate::pipeline_field::PipelineField;

/// Whether a kind of action takes a field of the pipeline: never, so data that gives it is
/// refused; when data gives it; or always, so data that does not give it is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Takes {
    Never,
    May,
    Must,
}

/// The pipeline's fields a kind of action takes, by field, which its section declares: a field
/// its kind does not run is one it never takes, so no data the kind ignores loads
/// (04-capabilities/actions.md, Each kind owns its section). A range a kind must take is in meters
/// at every rank; a kind takes a minimum range with any range, and its passive's hold only while
/// ready with its passive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindTakes([Takes; <PipelineField as NamedEnum>::ALL.len()]);

impl KindTakes {
    /// A kind that takes no field of the pipeline, which a kind's own names what it takes from.
    pub const NONE: Self = Self([Takes::Never; <PipelineField as NamedEnum>::ALL.len()]);

    /// A kind that may take every field of the pipeline, as a cast does.
    pub const ANY: Self = Self([Takes::May; <PipelineField as NamedEnum>::ALL.len()]);

    /// It with each of `fields` taken as `takes`; a field it takes with another, a minimum range
    /// or a passive's hold while ready, is a caller's error.
    #[must_use]
    pub const fn with(self, takes: Takes, fields: &[PipelineField]) -> Self {
        let mut held = self.0;
        let mut at = 0;
        while at < fields.len() {
            let field = fields[at];
            assert!(!field.follows(), "a field another's take decides");
            held[field as usize] = takes;
            at += 1;
        }
        Self(held)
    }

    /// Whether it takes `field`.
    pub const fn of(self, field: PipelineField) -> Takes {
        match field {
            PipelineField::MinRange => match self.0[PipelineField::Range as usize] {
                Takes::Never => Takes::Never,
                Takes::May | Takes::Must => Takes::May,
            },
            PipelineField::PassiveWhileReady => self.0[PipelineField::PassiveModifier as usize],
            _ => self.0[field as usize],
        }
    }

    /// Whether its range is in meters at every rank, as it must take one.
    pub const fn metered(self) -> bool {
        matches!(self.0[PipelineField::Range as usize], Takes::Must)
    }
}
