use campfire_geometry::Shape;
use campfire_math::{Num, Vec3};
use campfire_script::{
    ApiBuilder, ApiError, ApiOwner, Checked, Ctx, MemberSpec, NumError, Raised, ScriptValue, Unit,
};
use campfire_sim::Position;

use crate::units::avatar::Avatar;
use crate::units::collision_data::CollisionData;
use crate::units::tag_data::TagData;
use crate::units::unit_state_access::UnitStateAccess;

/// The script API of positions and vectors: `Pos` with `distance_to`, `within` and
/// `direction_to`, and `Vector` with `rotated_deg`; and the fields of `Unit` that read the core's
/// units beside the view's row: `is_avatar` and `state`.
#[derive(Debug)]
pub struct UnitsApi;

impl UnitsApi {
    /// `Pos`, `Vector`, and `Unit`'s `is_avatar` and `state`.
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let field = |name, description| MemberSpec::field(ApiOwner::Unit, name, description);
        api.bind(
            field("is_avatar", "whether it is an avatar"),
            |unit: &mut Unit| unit.read(Avatar::tagged),
        )
        .bind(
            field(
                "state",
                "its script state, by name, which a call may write and read back",
            ),
            |unit: &mut Unit| {
                let unit_type = unit.read(|row| row.unit_type);
                UnitStateAccess::of_row(unit.id, unit_type, unit.row_index())
            },
        );
        UnitStateAccess::register(api);
        api.data::<CollisionData>(&["radius", "box", "layer"], &[])
            .data::<TagData>(&["blocks", "hidden", "detects", "immune"], &[]);
        let position = |name, signature, description| {
            MemberSpec::method(ApiOwner::Position, name, signature, description)
        };
        let within = position(
            "within",
            &[&["pos", "radius"]],
            "whether `pos` is within `radius` in the map's metric, exactly: the reach rule between two points, which have no bodies",
        );
        api.ty::<Position>("Pos")
            .method(
                position(
                    "distance_to",
                    &[&["pos"]],
                    "the distance to `pos` in the map's metric",
                ),
                |ctx: &Ctx, from: Position, to: Position| {
                    ctx.view()
                        .metric()
                        .offset(from, to)
                        .checked_length()
                        .ok_or_else(|| Box::new(Raised::error(NumError::Overflow)))
                },
            )
            .method(
                within,
                |ctx: &Ctx, from: Position, to: Position, radius: Num| {
                    UnitsApi::within(ctx, from, to, radius)
                },
            )
            .method(
                position(
                    "direction_to",
                    &[&["pos"]],
                    "the unit vector towards `pos` in the map's metric, `()` for the same point",
                ),
                |ctx: &Ctx, from: Position, to: Position| {
                    Ok(ctx
                        .view()
                        .metric()
                        .offset(from, to)
                        .normalized()
                        .map_or(ScriptValue::UNIT, ScriptValue::of))
                },
            );
        let vector = |name, signature, description| {
            MemberSpec::method(ApiOwner::Vector, name, signature, description)
        };
        let rotated = vector(
            "rotated_deg",
            &[&["degrees"]],
            "the vector turned by `degrees` about the vertical, counter-clockwise seen from above",
        );
        api.ty::<Vec3>("Vector")
            .method(rotated, |vector: Vec3, degrees: Num| {
                UnitsApi::rotated(vector, degrees)
            });
        let scaled = MemberSpec::operator(
            ApiOwner::Vector,
            "*",
            "the vector scaled by a number, on either side",
        );
        api.method(
            MemberSpec::operator(ApiOwner::Vector, "+", "the sum of two vectors"),
            |a: Vec3, b: Vec3| UnitsApi::exact(a.checked_add(b)),
        )
        .method(
            MemberSpec::operator(ApiOwner::Vector, "-", "the difference of two vectors"),
            |a: Vec3, b: Vec3| UnitsApi::exact(a.checked_sub(b)),
        )
        .method(scaled, |vector: Vec3, factor: Num| {
            UnitsApi::exact(vector.checked_mul(factor))
        })
        .method(scaled, |factor: Num, vector: Vec3| {
            UnitsApi::exact(vector.checked_mul(factor))
        });
    }

    /// The vector a checked operation gave; one past what a number holds fails the call.
    fn exact(vector: Option<Vec3>) -> Checked<Vec3> {
        vector.ok_or_else(|| Box::new(Raised::error(NumError::Overflow)))
    }

    /// `vector` turned by `degrees` about the vertical; a turn too large to compute fails the call.
    fn rotated(vector: Vec3, degrees: Num) -> Checked<Vec3> {
        let radians = degrees
            .checked_mul(Num::PI)
            .and_then(|turn| turn.checked_div_int(180))
            .ok_or_else(|| Box::new(Raised::error(NumError::Overflow)))?;
        UnitsApi::exact(vector.checked_rotated_y(radians.sin_cos()))
    }

    /// Whether `to` is within `radius` of `from` in the map's metric, exactly: the reach rule
    /// between two points, which have no bodies.
    fn within(ctx: &Ctx, from: Position, to: Position, radius: Num) -> Checked<bool> {
        let radius = ApiError::radius(radius)?;
        let metric = ctx.view().metric();
        Ok(metric.reaches(from, Shape::POINT, radius, to, Shape::POINT))
    }
}
