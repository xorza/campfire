/// Whether a kind of action takes a field of the pipeline: never, so data that gives it is
/// refused; when data gives it; or always, so data that does not give it is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Takes {
    Never,
    May,
    Must,
}

/// Whether a kind of action takes a range: never; one data may give, global at a rank or in
/// meters, global when absent; or one data must give, in meters at every rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeTakes {
    Never,
    May,
    Meters,
}

/// The pipeline's fields a kind of action takes, which its section declares: a field its kind
/// does not run is one it never takes, so no data the kind ignores loads
/// (04-capabilities/actions.md, Each kind owns its section).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindTakes {
    pub script: Takes,
    pub range: RangeTakes,
    pub cooldown: Takes,
    pub cost: Takes,
    pub windup: Takes,
    pub clamp_to_range: Takes,
    pub toggle: Takes,
    pub channel: Takes,
    pub hold: Takes,
    pub charges: Takes,
    pub charge: Takes,
    /// Its passive modifier, and whether it holds only while ready.
    pub passive: Takes,
    pub delivery: Takes,
    pub params: Takes,
    pub on_resolve: Takes,
    pub on_hit: Takes,
    pub on_end: Takes,
}

impl KindTakes {
    /// A kind that takes no field of the pipeline, which a kind's own names what it takes from.
    pub const NONE: Self = Self {
        script: Takes::Never,
        range: RangeTakes::Never,
        cooldown: Takes::Never,
        cost: Takes::Never,
        windup: Takes::Never,
        clamp_to_range: Takes::Never,
        toggle: Takes::Never,
        channel: Takes::Never,
        hold: Takes::Never,
        charges: Takes::Never,
        charge: Takes::Never,
        passive: Takes::Never,
        delivery: Takes::Never,
        params: Takes::Never,
        on_resolve: Takes::Never,
        on_hit: Takes::Never,
        on_end: Takes::Never,
    };

    /// A kind that may take every field of the pipeline, as a cast does.
    pub const ANY: Self = Self {
        script: Takes::May,
        range: RangeTakes::May,
        cooldown: Takes::May,
        cost: Takes::May,
        windup: Takes::May,
        clamp_to_range: Takes::May,
        toggle: Takes::May,
        channel: Takes::May,
        hold: Takes::May,
        charges: Takes::May,
        charge: Takes::May,
        passive: Takes::May,
        delivery: Takes::May,
        params: Takes::May,
        on_resolve: Takes::May,
        on_hit: Takes::May,
        on_end: Takes::May,
    };
}
