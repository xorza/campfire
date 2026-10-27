use std::collections::BTreeMap;

use campfire_client_files::{Blend, EffectsFile, ListedSystem, ParticleSystem, SoundEvent};
use campfire_common::DeclaredName;
use campfire_values::PackagePath;

/// An effects file as the client plays it (02-engine-core.md, Effects are a format of the engine):
/// its step and its cap, each system by its id, the systems it names and the group it draws in
/// resolved once, as the file loads, each sound by its id, and each list by its id, each entry's
/// system and each of its sounds resolved (02-engine-core.md, A sound is an effect).
#[derive(Debug)]
pub(crate) struct EffectBook {
    step: f32,
    cap: u32,
    systems: Vec<BookSystem>,
    ids: BTreeMap<DeclaredName, SystemId>,
    groups: Vec<DrawGroup>,
    sounds: Vec<SoundEvent>,
    sound_ids: BTreeMap<DeclaredName, SoundId>,
    lists: Vec<Vec<BookEntry>>,
    list_sounds: Vec<Vec<SoundId>>,
    list_ids: BTreeMap<DeclaredName, ListId>,
}

/// An entry of a list: its file's, and its system's id.
#[derive(Debug)]
pub(crate) struct BookEntry {
    pub(crate) system: SystemId,
    pub(crate) spec: ListedSystem,
}

/// A list's place among a book's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ListId(u32);

/// A system of the book: its file's, its slave's id and its particles' system's, and the group
/// its particles draw in.
#[derive(Debug)]
pub(crate) struct BookSystem {
    pub(crate) spec: ParticleSystem,
    pub(crate) slave: Option<SystemId>,
    pub(crate) attached: Option<SystemId>,
    pub(crate) group: GroupId,
}

/// The particles of one texture drawn in one blend, which draw as one mesh.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DrawGroup {
    pub(crate) texture: PackagePath,
    pub(crate) blend: Blend,
}

/// A system's place among a book's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SystemId(u32);

/// A sound's place among a book's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SoundId(u32);

/// A draw group's place among a book's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct GroupId(u32);

impl EffectBook {
    /// The book of `file`, whose load resolved each name it holds.
    pub(crate) fn new(file: &EffectsFile) -> Self {
        let ids: BTreeMap<DeclaredName, SystemId> = file
            .systems()
            .keys()
            .enumerate()
            .map(|(at, name)| (name.clone(), SystemId::at(at)))
            .collect();
        let id = |name: &DeclaredName| ids[name];
        let mut groups: Vec<DrawGroup> = Vec::new();
        let systems = file
            .systems()
            .values()
            .map(|spec| {
                let group = DrawGroup {
                    texture: spec.texture.clone(),
                    blend: spec.blend,
                };
                let at = groups.iter().position(|known| *known == group);
                let at = at.unwrap_or_else(|| {
                    groups.push(group);
                    groups.len() - 1
                });
                BookSystem {
                    slave: spec.slave.as_ref().map(|slave| id(&slave.system)),
                    attached: spec.attached.as_ref().map(id),
                    group: GroupId(u32::try_from(at).expect("a book's groups fit u32")),
                    spec: spec.clone(),
                }
            })
            .collect();
        let sound_ids: BTreeMap<DeclaredName, SoundId> = file
            .sounds()
            .keys()
            .enumerate()
            .map(|(at, name)| (name.clone(), SoundId::at(at)))
            .collect();
        let list_sounds = file
            .lists()
            .values()
            .map(|list| list.sounds.iter().map(|name| sound_ids[name]).collect())
            .collect();
        let lists = file
            .lists()
            .values()
            .map(|list| {
                let entries = list.systems.iter().map(|entry| BookEntry {
                    system: id(&entry.system),
                    spec: entry.clone(),
                });
                entries.collect()
            })
            .collect();
        let list_ids = file
            .lists()
            .keys()
            .enumerate()
            .map(|(at, name)| {
                let at = u32::try_from(at).expect("a book's lists fit u32");
                (name.clone(), ListId(at))
            })
            .collect();
        Self {
            step: file.step(),
            cap: file.cap(),
            systems,
            ids,
            groups,
            sounds: file.sounds().values().cloned().collect(),
            sound_ids,
            lists,
            list_sounds,
            list_ids,
        }
    }

    /// The id of the sound of `name`; none for a name the file does not hold.
    pub(crate) fn sound_id(&self, name: &DeclaredName) -> Option<SoundId> {
        self.sound_ids.get(name).copied()
    }

    /// The sound of `id`.
    pub(crate) fn sound(&self, id: SoundId) -> &SoundEvent {
        &self.sounds[id.index()]
    }

    /// Its sounds, by their ids' order.
    pub(crate) fn sounds(&self) -> &[SoundEvent] {
        &self.sounds
    }

    /// The sounds the list `id` plays.
    pub(crate) fn list_sounds(&self, id: ListId) -> &[SoundId] {
        let ListId(at) = id;
        &self.list_sounds[usize::try_from(at).expect("a u32 fits usize")]
    }

    /// The id of the list of `name`; none for a name the file does not hold.
    pub(crate) fn list_id(&self, name: &DeclaredName) -> Option<ListId> {
        self.list_ids.get(name).copied()
    }

    /// The entries of the list `id`.
    pub(crate) fn list(&self, id: ListId) -> &[BookEntry] {
        let ListId(at) = id;
        &self.lists[usize::try_from(at).expect("a u32 fits usize")]
    }

    /// The seconds each step lasts.
    pub(crate) const fn step(&self) -> f32 {
        self.step
    }

    /// The most particles alive at once.
    pub(crate) const fn cap(&self) -> u32 {
        self.cap
    }

    /// The system of `id`.
    pub(crate) fn system(&self, id: SystemId) -> &BookSystem {
        &self.systems[id.index()]
    }

    /// The id of the system of `name`; none for a name the file does not hold.
    pub(crate) fn id(&self, name: &DeclaredName) -> Option<SystemId> {
        self.ids.get(name).copied()
    }

    /// Its draw groups, by their ids' order.
    pub(crate) fn groups(&self) -> &[DrawGroup] {
        &self.groups
    }
}

impl SystemId {
    fn at(at: usize) -> Self {
        Self(u32::try_from(at).expect("a book's systems fit u32"))
    }

    pub(crate) fn index(self) -> usize {
        usize::try_from(self.0).expect("a u32 fits usize")
    }
}

impl SoundId {
    fn at(at: usize) -> Self {
        Self(u32::try_from(at).expect("a book's sounds fit u32"))
    }

    pub(crate) fn index(self) -> usize {
        usize::try_from(self.0).expect("a u32 fits usize")
    }
}

impl GroupId {
    /// The group at `at` among a book's.
    pub(crate) fn of(at: usize) -> Self {
        Self(u32::try_from(at).expect("a book's groups fit u32"))
    }

    pub(crate) fn index(self) -> usize {
        usize::try_from(self.0).expect("a u32 fits usize")
    }
}
