use std::ops::Range;

use bevy_ecs::resource::Resource;
use campfire_math::Num;

use crate::units::action_id::ActionId;
use crate::units::body::BodyForm;
use crate::units::filter::Filter;
use crate::values::share::Share;

/// What each build places and how its site grows, by action, its rates and its placement's
/// rules one run after another: package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BuildSpecs {
    /// By action, as the book loads them in order.
    specs: Vec<Stored>,
    rates: Vec<Num>,
    rules: Vec<PlacementCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Stored {
    action: ActionId,
    form: BodyForm,
    style: StoredStyle,
    start_life: Option<Share>,
    refund: Share,
    near: Range<u32>,
    away: Range<u32>,
}

/// How a site grows: by itself, while one builder builds it, or at its builders' rate, from the
/// rate for one builder, one rate at least.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Style<'a> {
    Alone,
    Builder,
    Builders(&'a [Num]),
}

/// A `Style` as `BuildSpecs` holds it: its builders' rates a run of its rates.
#[derive(Debug, Clone, PartialEq, Eq)]
enum StoredStyle {
    Alone,
    Builder,
    Builders(Range<u32>),
}

/// An entry of a placement: a living unit the filter selects for the builder, within `distance`
/// of the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlacementCheck {
    pub(crate) filter: Filter,
    pub(crate) distance: Num,
}

/// One build's spec, as `BuildSpecs` holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BuildSpec<'a> {
    /// The building's box, before its angle turns it.
    pub(crate) form: BodyForm,
    pub(crate) style: Style<'a>,
    /// The share of its life pool's maximum a site starts with, none for a type without it.
    pub(crate) start_life: Option<Share>,
    /// The share of what the build paid that a cancel returns.
    pub(crate) refund: Share,
    pub(crate) near: &'a [PlacementCheck],
    pub(crate) away: &'a [PlacementCheck],
}

/// What one build loads with.
#[derive(Debug, Clone)]
pub(crate) struct NewBuild<'a> {
    pub(crate) form: BodyForm,
    pub(crate) style: Style<'a>,
    pub(crate) start_life: Option<Share>,
    pub(crate) refund: Share,
    pub(crate) near: Vec<PlacementCheck>,
    pub(crate) away: Vec<PlacementCheck>,
}

impl BuildSpecs {
    /// Gives `action`, loaded after every action it holds, its build.
    pub(crate) fn push(&mut self, action: ActionId, build: NewBuild<'_>) {
        debug_assert!(self.specs.last().is_none_or(|last| last.action < action));
        let run = |at: usize| u32::try_from(at).expect("a book's builds fit a u32");
        let style = match build.style {
            Style::Alone => StoredStyle::Alone,
            Style::Builder => StoredStyle::Builder,
            Style::Builders(rates) => {
                assert!(!rates.is_empty(), "a builders style has a rate for one");
                let start = run(self.rates.len());
                self.rates.extend_from_slice(rates);
                StoredStyle::Builders(start..run(self.rates.len()))
            }
        };
        let near = run(self.rules.len());
        self.rules.extend(build.near);
        let away = run(self.rules.len());
        self.rules.extend(build.away);
        self.specs.push(Stored {
            action,
            form: build.form,
            style,
            start_life: build.start_life,
            refund: build.refund,
            near: near..away,
            away: away..run(self.rules.len()),
        });
    }

    /// The build of `action`; `None` for an action that builds nothing.
    pub(crate) fn of(&self, action: ActionId) -> Option<BuildSpec<'_>> {
        let at = self
            .specs
            .binary_search_by_key(&action, |spec| spec.action)
            .ok()?;
        let spec = &self.specs[at];
        let range = |run: &Range<u32>| run.start as usize..run.end as usize;
        let style = match &spec.style {
            StoredStyle::Alone => Style::Alone,
            StoredStyle::Builder => Style::Builder,
            StoredStyle::Builders(rates) => Style::Builders(&self.rates[range(rates)]),
        };
        Some(BuildSpec {
            form: spec.form,
            style,
            start_life: spec.start_life,
            refund: spec.refund,
            near: &self.rules[range(&spec.near)],
            away: &self.rules[range(&spec.away)],
        })
    }
}

impl BuildSpec<'_> {
    /// The progress a tick adds to a site with `builders` builders that build it: 1 for `alone`;
    /// for `builder`, 1 with one or more, else 0; for `builders`, the table's entry for the
    /// count, the last for a count past it, and 0 for none.
    pub(crate) fn rate(self, builders: usize) -> Num {
        match (self.style, builders) {
            (Style::Builder | Style::Builders(_), 0) => Num::ZERO,
            (Style::Alone | Style::Builder, _) => Num::ONE,
            (Style::Builders(rates), count) => rates[(count - 1).min(rates.len() - 1)],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_style_gives_its_rate_for_each_count_of_builders() {
        // Rates 1, 1.5 and 2 for one, two and three builders, the last for any count past it.
        let rates = [Num::ONE, Num::ONE + Num::HALF, Num::int(2)];
        let mut specs = BuildSpecs::default();
        for (at, style) in [Style::Alone, Style::Builder, Style::Builders(&rates)]
            .into_iter()
            .enumerate()
        {
            let build = NewBuild {
                form: BodyForm::boxed([Num::ONE, Num::ONE]).unwrap(),
                style,
                start_life: None,
                refund: Share::ALL,
                near: Vec::new(),
                away: Vec::new(),
            };
            specs.push(ActionId::new(u32::try_from(at).unwrap() * 2), build);
        }
        let rate = |action, builders| specs.of(ActionId::new(action)).unwrap().rate(builders);
        let counts = [0, 1, 2, 3, 7];
        assert_eq!(counts.map(|count| rate(0, count)), [Num::ONE; 5]);
        assert_eq!(
            counts.map(|count| rate(2, count)),
            [Num::ZERO, Num::ONE, Num::ONE, Num::ONE, Num::ONE]
        );
        assert_eq!(
            counts.map(|count| rate(4, count)),
            [Num::ZERO, rates[0], rates[1], rates[2], rates[2]]
        );
        assert!(specs.of(ActionId::new(1)).is_none());
    }
}
