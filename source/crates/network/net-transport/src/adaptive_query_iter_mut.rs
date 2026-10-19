use bevy_ecs::query::{IterQueryData, QueryFilter, QueryItem};
use bevy_ecs::system::Query;

/// A mutable query iterated serially when it holds at most one item, and in parallel otherwise: a
/// parallel iteration costs more than it saves on a single item (13-net-stack/layering.md, The
/// iteration over links is `net-transport`'s).
#[derive(Debug)]
pub struct AdaptiveQueryIterMut<'query, 'world, 'state, D, F>
where
    D: IterQueryData,
    F: QueryFilter,
{
    query: &'query mut Query<'world, 'state, D, F>,
}

impl<'query, 'world, 'state, D, F> AdaptiveQueryIterMut<'query, 'world, 'state, D, F>
where
    D: IterQueryData,
    F: QueryFilter,
{
    const SERIAL_THRESHOLD: usize = 1;

    pub const fn new(query: &'query mut Query<'world, 'state, D, F>) -> Self {
        Self { query }
    }

    /// Applies `func` to every matching item.
    pub fn for_each<Func>(self, func: Func)
    where
        Func: for<'item> Fn(QueryItem<'item, 'state, D>) + Send + Sync + Clone,
    {
        if self.query.iter().nth(Self::SERIAL_THRESHOLD).is_some() {
            self.query.par_iter_mut().for_each(func);
        } else {
            self.query.iter_mut().for_each(func);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_app::{App, TaskPoolPlugin};
    use bevy_ecs::prelude::Component;

    #[derive(Debug, Component)]
    struct Value(u32);

    /// One item runs serially and two in parallel; both visit every item once.
    #[test]
    fn adaptive_iteration_visits_every_item_once_serially_or_in_parallel() {
        let mut app = App::new();
        app.add_plugins(TaskPoolPlugin::default());
        let first = app.world_mut().spawn(Value(0)).id();
        let mut query_state = app.world_mut().query::<&mut Value>();

        {
            let mut query = query_state.query_mut(app.world_mut());
            AdaptiveQueryIterMut::new(&mut query).for_each(|mut value| value.0 += 1);
        }
        let second = app.world_mut().spawn(Value(0)).id();
        {
            let mut query = query_state.query_mut(app.world_mut());
            AdaptiveQueryIterMut::new(&mut query).for_each(|mut value| value.0 += 1);
        }

        assert_eq!(app.world().get::<Value>(first).unwrap().0, 2);
        assert_eq!(app.world().get::<Value>(second).unwrap().0, 1);
    }
}
