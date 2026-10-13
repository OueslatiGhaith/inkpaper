//! StarDict dictionaries on the card, found like crosspoint's
//! `DictionaryRegistry`: a folder each.

use alloc::{format, string::String, vec::Vec};

use inkpaper_dictionary::{Definition, Dictionary, DictionaryInfo};

use crate::{AppPlatform, PlatformEntry, fonts::hidden};

/// Scanned in order, so a dictionary in `/.dictionaries` hides one of the same
/// name in `/dictionaries`.
const DICTIONARY_ROOTS: [&str; 2] = ["/.dictionaries", "/dictionaries"];

/// As many as the picker fits on the screen.
const MAX_DICTIONARIES: usize = 10;

/// An `.ifo` is a few lines of text.
const MAX_INFO_BYTES: usize = 16 * 1024;

/// A dictionary on the card, named after its folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DictionaryFolder {
    name: String,
    /// paths of its `.ifo`, `.idx` and `.dict` or `.dict.dz`
    info: String,
    index: String,
    data: String,
    /// the path of its `.syn`, when it has one
    synonyms: Option<String>,
}

impl DictionaryFolder {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// What its index samples are saved as: a state name can't hold every
    /// folder name, so a hash of it.
    pub(crate) fn samples_state(&self) -> String {
        // FNV-1a
        let hash = self
            .name
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
            });

        format!("dictionary-{hash:016x}.dat")
    }

    /// Opens its files, or `None` when one can't be read or the `.ifo` isn't
    /// StarDict's.
    pub(crate) async fn open<P: AppPlatform>(
        &self,
        platform: &mut P,
    ) -> Option<Dictionary<P::RandomAccessSource>> {
        let info = platform.read_file(&self.info, MAX_INFO_BYTES).await.ok()?;
        let info = DictionaryInfo::parse(&info).ok()?;

        let index = platform.open_random_access(&self.index).await.ok()?;
        let data = platform.open_random_access(&self.data).await.ok()?;

        let mut dictionary = Dictionary::open(info, index, data).await.ok()?;

        // a dictionary still works without its synonyms
        if let Some(path) = &self.synonyms
            && let Ok(synonyms) = platform.open_random_access(path).await
        {
            let _ = dictionary.use_synonyms(synonyms).await;
        }

        Some(dictionary)
    }
}

/// The dictionary folders, sorted by name.
pub(crate) async fn find_dictionaries<P: AppPlatform>(platform: &mut P) -> Vec<DictionaryFolder> {
    let mut dictionaries: Vec<DictionaryFolder> = Vec::new();

    for root in DICTIONARY_ROOTS {
        // a card without the folder has no dictionaries in it
        let Ok(entries) = platform.list_directory(root).await else {
            continue;
        };

        for entry in entries {
            let name = entry.name();

            if !entry.is_directory()
                || hidden(name)
                || dictionaries.iter().any(|other| other.name == name)
            {
                continue;
            }

            let folder = format!("{root}/{name}");

            let Ok(files) = platform.list_directory(&folder).await else {
                continue;
            };

            if let Some(dictionary) = dictionary_folder(name, &folder, &files) {
                dictionaries.push(dictionary);
            }
        }
    }

    dictionaries.sort_by(|left, right| left.name.cmp(&right.name));
    dictionaries.truncate(MAX_DICTIONARIES);

    dictionaries
}

/// The one dictionary among a folder's `files`: an `.ifo`, an `.idx`, and a
/// `.dict` or `.dict.dz` sharing a name, in any case, and maybe a `.syn`.
/// Like crosspoint, a folder of several dictionaries is skipped as ambiguous.
fn dictionary_folder(
    name: &str,
    folder: &str,
    files: &[PlatformEntry],
) -> Option<DictionaryFolder> {
    let files = || {
        files
            .iter()
            .filter(|entry| !entry.is_directory() && !hidden(entry.name()))
            .map(PlatformEntry::name)
    };

    let mut indexes = files().filter(|file| strip_extension(file, ".idx").is_some());

    let index = indexes.next()?;

    if indexes.next().is_some() {
        return None;
    }

    let stem = strip_extension(index, ".idx")?;
    let file = |extension: &str| {
        files()
            .find(|file| strip_extension(file, extension) == Some(stem))
            .map(|file| format!("{folder}/{file}"))
    };

    Some(DictionaryFolder {
        name: String::from(name),
        info: file(".ifo")?,
        index: format!("{folder}/{index}"),
        // the plain one reads without inflating
        data: file(".dict").or_else(|| file(".dict.dz"))?,
        synonyms: file(".syn"),
    })
}

/// The name without `extension`, in any case.
fn strip_extension<'a>(name: &'a str, extension: &str) -> Option<&'a str> {
    let split = name.len().checked_sub(extension.len())?;
    let (stem, tail) = (name.get(..split)?, name.get(split..)?);

    (!stem.is_empty() && tail.eq_ignore_ascii_case(extension)).then_some(stem)
}

/// A word to look up, and in which of the dictionaries found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DictionaryRequest {
    pub dictionary: u8,
    pub word: String,
}

/// What looking a word up came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LookupOutcome {
    Found(Definition),
    NotFound,
    /// the dictionary couldn't be opened or read
    Failed,
}

/// A word being looked up, or its outcome, for the reader to show.
#[derive(Debug, Default)]
pub(crate) struct WordLookup {
    request: Option<DictionaryRequest>,
    /// the word looked up last, and its outcome once the service has it
    current: Option<(String, Option<LookupOutcome>)>,
}

impl WordLookup {
    /// Looks `word` up in `dictionary`.
    pub(crate) fn start(&mut self, dictionary: u8, word: String) {
        self.current = Some((word.clone(), None));
        self.request = Some(DictionaryRequest { dictionary, word });
    }

    pub(crate) fn take_request(&mut self) -> Option<DictionaryRequest> {
        self.request.take()
    }

    /// The service's outcome for `word`. Returns whether it's the word still
    /// wanted.
    pub(crate) fn finish(&mut self, word: &str, outcome: LookupOutcome) -> bool {
        match &mut self.current {
            Some((current, slot)) if current == word => {
                *slot = Some(outcome);
                true
            }
            _ => false,
        }
    }
}

/// The dictionaries Settings offers, and whether its picker is open.
#[derive(Debug, Default)]
pub(crate) struct DictionarySettings {
    names: Vec<String>,
    picker_open: bool,
}

impl DictionarySettings {
    /// Returns whether the names changed.
    pub(crate) fn set_names(&mut self, names: Vec<String>) -> bool {
        if self.names == names {
            return false;
        }

        self.names = names;

        true
    }

    pub(crate) fn names(&self) -> &[String] {
        &self.names
    }

    pub(crate) const fn picker_open(&self) -> bool {
        self.picker_open
    }

    /// Returns whether the picker opened.
    pub(crate) fn open_picker(&mut self) -> bool {
        !core::mem::replace(&mut self.picker_open, true)
    }

    /// Returns whether the picker closed.
    pub(crate) fn close_picker(&mut self) -> bool {
        core::mem::take(&mut self.picker_open)
    }

    /// The picker's options: none, then each dictionary.
    pub(crate) fn options(&self) -> Vec<String> {
        core::iter::once(String::from("None"))
            .chain(self.names.iter().cloned())
            .collect()
    }
}
