use std::sync::Arc;

use campfire_declarations::{BookCx, BookError, BookPlace, NamesListError, Section, Sections};
use campfire_geometry::Polygon;
use campfire_math::Num;
use campfire_units::{DeclaredTags, Layer, MapBook, MapPoint, Wall, WallBook};
use campfire_values::{Names, NamesError, PackageIndex};

use crate::Navigation;
use crate::error::NavigationDataError;
use crate::map_navigation_data::MapNavigationData;
use crate::mode_navigation_data::ModeNavigationData;

impl Navigation {
    /// The rule of a mode that declares navigation: its map has a pathing grid.
    pub(crate) fn pathing_rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        if cx.map_section::<MapNavigationData>().is_none() {
            let at = BookPlace::MapSection(MapNavigationData::KEY);
            let problem = NavigationDataError::NoPathingGrid;
            return Err(BookError::rule(PackageIndex::MODE, at, problem));
        }
        Ok(())
    }

    /// Adds the names of the mode's layers, each a tag.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn layer_tags(cx: &BookCx<'_>, tags: &mut DeclaredTags) -> Result<(), BookError> {
        let data = cx.mode_section::<ModeNavigationData>().into_iter();
        tags.extend(data.flat_map(|data| &data.layers));
        Ok(())
    }

    /// The names of the mode's layers; an error for a name twice, or past the most layers a match
    /// holds.
    pub(crate) fn layer_names(cx: &BookCx<'_>) -> Result<Arc<Names<Layer>>, BookError> {
        let mut names = Names::default();
        let data = cx.mode_section::<ModeNavigationData>().into_iter();
        for name in data.flat_map(|data| &data.layers) {
            names.try_add(name.as_str()).map_err(|error| {
                let error = match error {
                    NamesError::Full => NamesListError::TooMany,
                    NamesError::Repeated => NamesListError::Repeated(name.clone()),
                };
                let at = BookPlace::Section(ModeNavigationData::KEY);
                BookError::rule(PackageIndex::MODE, at, error)
            })?;
        }
        Ok(Arc::new(names))
    }

    /// The map's walls, each on the layer it names, which the map's `[navigation]` holds; an
    /// error for a wall on a layer the mode does not declare, points that make no simple
    /// polygon, or a point that has no place on the map.
    pub fn walls(cx: &BookCx<'_>) -> Result<WallBook, BookError> {
        let map = cx.book::<MapBook>()?;
        let layers = cx.names_if::<Layer>()?.map(|layers| &**layers);
        let walls = Self::walls_of(map, cx.source().map, layers);
        walls.map_err(|problem| BookError::rule(PackageIndex::MODE, BookPlace::Map, problem))
    }

    /// The walls of a map of `sections`, as `map` places their points, on the layers `layers`
    /// names, none but the first without them; an error as `walls` gives.
    pub fn walls_of(
        map: &MapBook,
        sections: &Sections,
        layers: Option<&Names<Layer>>,
    ) -> Result<WallBook, NavigationDataError> {
        let position = |point: &MapPoint| {
            map.checked_position(*point)
                .map_err(NavigationDataError::Point)
        };
        let data = sections
            .get::<MapNavigationData>()
            .into_iter()
            .flat_map(|navigation| &navigation.walls);
        let mut walls = Vec::new();
        for (wall, data) in data.enumerate() {
            let layer = match &data.layer {
                Some(name) => {
                    let named = layers.and_then(|layers| layers.named(name.as_str()));
                    named.ok_or_else(|| NavigationDataError::UnknownLayer(name.clone()))?
                }
                None => Layer::FIRST,
            };
            let plane = data.points.iter().map(|point| {
                let at = position(point)?;
                Ok([at.x, at.z])
            });
            let plane = plane.collect::<Result<Vec<[Num; 2]>, NavigationDataError>>()?;
            let area = Polygon::new(plane)
                .map_err(|problem| NavigationDataError::Wall { wall, problem })?;
            walls.push(Wall { layer, area });
        }
        Ok(WallBook::new(&walls))
    }
}
