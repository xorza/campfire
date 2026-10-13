use campfire_math::Num;

/// The widest a body's bound may be, a circle's radius or the farthest corner of a box, 2³⁵ bits:
/// the power of two past Zero Hour's widest object, and small enough that two bodies' bounds and a
/// range add up within a `Num`.
pub const MAX_BOUND: Num = Num::int(2048);
