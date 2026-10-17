use std::any::{Any, TypeId};
use std::borrow::Cow;
use std::fmt;

use crate::section::Section;

/// The sections a data table gives, each read as its module's type.
#[derive(Default)]
pub struct Sections(Vec<Box<dyn SectionValue>>);

/// A section as the table holds it, whatever its type.
pub(crate) trait SectionValue: Any + Send + Sync {
    fn key(&self) -> &'static str;

    fn as_any(&self) -> &dyn Any;

    #[cfg(feature = "internals")]
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn clone_box(&self) -> Box<dyn SectionValue>;

    fn eq_dyn(&self, other: &dyn SectionValue) -> bool;

    fn fmt_value(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;
}

impl<S: Section> SectionValue for S {
    fn key(&self) -> &'static str {
        S::KEY
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    #[cfg(feature = "internals")]
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn SectionValue> {
        Box::new(self.clone())
    }

    fn eq_dyn(&self, other: &dyn SectionValue) -> bool {
        other.as_any().downcast_ref::<S>() == Some(self)
    }

    fn fmt_value(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl Sections {
    /// Its section `S`, if the table gives it.
    pub fn get<S: Section>(&self) -> Option<&S> {
        self.0
            .iter()
            .find_map(|section| section.as_any().downcast_ref())
    }

    /// Its section `S`, or the value a table that does not give it reads as.
    pub fn get_or_default<S: Section + Default>(&self) -> Cow<'_, S> {
        self.get()
            .map_or_else(|| Cow::Owned(S::default()), Cow::Borrowed)
    }

    /// Whether the table gives `S`.
    pub fn holds<S: Section>(&self) -> bool {
        self.get::<S>().is_some()
    }

    /// The keys of the sections it gives.
    pub fn keys(&self) -> impl ExactSizeIterator<Item = &'static str> + Clone {
        self.0.iter().map(|section| section.key())
    }

    /// It with `section` too, in place of a section of its type that it gives.
    #[must_use]
    pub fn with<S: Section>(mut self, section: S) -> Self {
        self.0
            .retain(|held| held.as_any().type_id() != TypeId::of::<S>());
        self.0.push(Box::new(section));
        self
    }

    pub(crate) fn push(&mut self, section: Box<dyn SectionValue>) {
        self.0.push(section);
    }

    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Clone for Sections {
    fn clone(&self) -> Self {
        Self(self.0.iter().map(|section| section.clone_box()).collect())
    }
}

/// Two give the same sections, each equal, in any order.
impl PartialEq for Sections {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self
                .0
                .iter()
                .all(|section| (other.0.iter()).any(|held| section.eq_dyn(&**held)))
    }
}

impl Eq for Sections {}

/// Each section by its key.
impl fmt::Debug for Sections {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut map = f.debug_map();
        for section in &self.0 {
            map.key(&section.key());
            map.value(&fmt::from_fn(|f| section.fmt_value(f)));
        }
        map.finish()
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::section::Section;
    use crate::sections::Sections;

    impl Sections {
        /// Its section `S`, which a test changes in place, if the table gives it.
        pub fn get_mut<S: Section>(&mut self) -> Option<&mut S> {
            (self.0.iter_mut()).find_map(|section| section.as_any_mut().downcast_mut())
        }
    }
}
