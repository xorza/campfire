use campfire_geometry::Placed;
use campfire_math::{Num, SinCos, Vec3};
use campfire_script::{
    ApiBuilder, ApiHolder, Checked, Ctx, MemberDecl, NumError, Raised, ScriptPosition,
    ScriptRadius, ScriptValue, Unit, UnitAt,
};

use crate::avatar::Avatar;
use crate::collision_data::CollisionData;
use crate::tag_data::TagData;
use crate::unit_state_access::UnitStateAccess;

/// The script API of vectors, `Vector`, with `rotated_deg`, and `distance_to`, `within` and
/// `direction_to` between two places; and the fields of `Unit` that read the core's
/// units beside the view's row: `is_avatar` and `state`.
#[derive(Debug)]
pub struct UnitsApi;

impl UnitsApi {
    /// `Vector`, and `Unit`'s `is_avatar` and `state`.
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let field =
            |name, description| MemberDecl::field(ApiHolder::of::<Unit>(), name, description);
        api.bind(
            field("is_avatar", "whether it is an avatar"),
            |unit: &mut Unit| unit.read(Avatar::is_tagged),
        )
        .bind(
            field(
                "state",
                "its script state, by name, which a call may write and read back",
            ),
            |unit: &mut Unit| match unit.at() {
                UnitAt::Row(row) => {
                    let unit_type = unit.view().read_row(row, |row| row.unit_type);
                    UnitStateAccess::of_row(unit.id, unit_type, row)
                }
                UnitAt::Created(unit_type) => UnitStateAccess::of_new(unit.id, unit_type),
            },
        );
        UnitStateAccess::register(api);
        api.data::<CollisionData>(&[]).data::<TagData>(&[]);
        let vector = |name, signature, description| {
            MemberDecl::method(ApiHolder::of::<Vec3>(), name, signature, description)
        };
        let within = vector(
            "within",
            &[&["position", "radius"]],
            "whether `position` is within `radius` in the map's metric, exactly: the reach rule between two points, which have no bodies",
        );
        let distance = vector(
            "distance_to",
            &[&["position"]],
            "the distance to `position` in the map's metric",
        );
        let direction = vector(
            "direction_to",
            &[&["position"]],
            "the unit vector towards `position` in the map's metric, `()` for the same point",
        );
        let rotated = vector(
            "rotated_deg",
            &[&["degrees"]],
            "the vector turned by `degrees` about the vertical, counter-clockwise seen from above",
        );
        api.ty::<Vec3>()
            .method(rotated, |vector: Vec3, degrees: Num| {
                Self::rotated(vector, degrees)
            })
            .method(
                distance,
                |ctx: &Ctx, from: ScriptPosition, to: ScriptPosition| {
                    ctx.view()
                        .metric()
                        .offset(from.get(), to.get())
                        .checked_length()
                        .ok_or_else(|| Box::new(Raised::error(NumError::Overflow)))
                },
            )
            .method(
                within,
                |ctx: &Ctx, from: ScriptPosition, to: ScriptPosition, radius: ScriptRadius| {
                    Ok(Self::within(ctx, from.get(), to.get(), radius))
                },
            )
            .method(
                direction,
                |ctx: &Ctx, from: ScriptPosition, to: ScriptPosition| {
                    Ok(ctx
                        .view()
                        .metric()
                        .offset(from.get(), to.get())
                        .normalized()
                        .map_or(ScriptValue::UNIT, ScriptValue::of))
                },
            );
        let scaled = MemberDecl::operator(
            ApiHolder::of::<Vec3>(),
            "*",
            "the vector scaled by a number, on either side",
        );
        api.method(
            MemberDecl::operator(ApiHolder::of::<Vec3>(), "+", "the sum of two vectors"),
            |a: Vec3, b: Vec3| Self::exact(a.checked_add(b)),
        )
        .method(
            MemberDecl::operator(
                ApiHolder::of::<Vec3>(),
                "-",
                "the difference of two vectors",
            ),
            |a: Vec3, b: Vec3| Self::exact(a.checked_sub(b)),
        )
        .method(scaled, |vector: Vec3, factor: Num| {
            Self::exact(vector.checked_mul(factor))
        })
        .method(scaled, |factor: Num, vector: Vec3| {
            Self::exact(vector.checked_mul(factor))
        })
        .method(
            MemberDecl::operator(
                ApiHolder::of::<Vec3>(),
                "==",
                "whether the two are one vector",
            ),
            |a: Vec3, b: Vec3| Ok(a == b),
        )
        .method(
            MemberDecl::operator(
                ApiHolder::of::<Vec3>(),
                "!=",
                "whether the two are two vectors",
            ),
            |a: Vec3, b: Vec3| Ok(a != b),
        );
        let component =
            |name, description| MemberDecl::field(ApiHolder::of::<Vec3>(), name, description);
        api.bind(
            component("x", "its first component, across the ground"),
            |vector: &mut Vec3| vector.x,
        )
        .bind(component("y", "its height"), |vector: &mut Vec3| vector.y)
        .bind(
            component("z", "its third component, across the ground"),
            |vector: &mut Vec3| vector.z,
        );
    }

    /// The vector a checked operation gave; one past what a number holds fails the call.
    fn exact(vector: Option<Vec3>) -> Checked<Vec3> {
        vector.ok_or_else(|| Box::new(Raised::error(NumError::Overflow)))
    }

    /// `vector` turned by `degrees` about the vertical, any number of degrees, by the sine and
    /// cosine of the angle each rounded once; a vector turned past what a number holds fails the
    /// call.
    fn rotated(vector: Vec3, degrees: Num) -> Checked<Vec3> {
        Self::exact(vector.checked_rotated_y(SinCos::of_degrees(degrees)))
    }

    /// Whether `to` is within `radius` of `from` in the map's metric, exactly: the reach rule
    /// between two points, which have no bodies.
    fn within(ctx: &Ctx, from: Vec3, to: Vec3, radius: ScriptRadius) -> bool {
        let metric = ctx.view().metric();
        metric.reaches(Placed::point(from), radius.get(), Placed::point(to))
    }
}
