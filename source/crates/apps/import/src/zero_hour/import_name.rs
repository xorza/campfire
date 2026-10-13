use campfire_capabilities::DeclaredName;
use campfire_common::MapName;

/// A name of the original, a map's folder or an object's template, as a package names it: its
/// ASCII letters lowercase, its digits and `_` kept, each other byte `_`, and `x` before a name
/// that does not start with a letter. Two names may give one, which the import refuses.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ImportName(String);

impl ImportName {
    pub(crate) fn of(original: &[u8]) -> ImportName {
        let mut name: String = original
            .iter()
            .map(|&byte| match byte.to_ascii_lowercase() {
                kept @ (b'a'..=b'z' | b'0'..=b'9') => char::from(kept),
                _ => '_',
            })
            .collect();
        if !name.starts_with(|first: char| first.is_ascii_lowercase()) {
            name.insert(0, 'x');
        }
        ImportName(name)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn declared(&self) -> DeclaredName {
        DeclaredName::new(&self.0).expect("an import name is a declared name")
    }

    pub(crate) fn map(&self) -> MapName {
        MapName::new(&self.0).expect("an import name is a map name")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_lowercases_and_replaces_what_a_declared_name_lacks() {
        for (original, name) in [
            (&b"AmericaTankCrusader"[..], "americatankcrusader"),
            (b"tournament desert", "tournament_desert"),
            (b"usa07-taskforces", "usa07_taskforces"),
            (b"*Waypoints/Waypoint", "x_waypoints_waypoint"),
            (b"_art review", "x_art_review"),
            (b"3v3", "x3v3"),
            (b"Grav\xFEl", "grav_l"),
            (b"", "x"),
        ] {
            let imported = ImportName::of(original);
            assert_eq!(imported.as_str(), name);
            assert_eq!(imported.declared().as_str(), name);
            assert_eq!(imported.map().as_str(), name);
        }
    }
}
