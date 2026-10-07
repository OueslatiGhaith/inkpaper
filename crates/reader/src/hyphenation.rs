use hypher::Lang;

/// A language the reader can hyphenate. Each comes from a `hypher` feature in
/// the workspace's `Cargo.toml`, so supporting another is one feature away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HyphenationLanguage(Lang);

impl HyphenationLanguage {
    pub const ENGLISH: Self = Self(Lang::English);

    /// The language of a tag like `en` or `en-US`, or `None` without patterns
    /// for it.
    pub fn from_tag(tag: &str) -> Option<Self> {
        let primary = tag.trim().split(['-', '_']).next()?;

        let &[first, second] = primary.as_bytes() else {
            return None;
        };

        Lang::from_iso([first.to_ascii_lowercase(), second.to_ascii_lowercase()]).map(Self)
    }

    /// Letters kept on each side of a break. English keeps crosspoint's 3,
    /// other languages their patterns' own.
    fn bounds(self) -> (usize, usize) {
        match self.0 {
            Lang::English => (3, 3),
            lang => lang.bounds(),
        }
    }

    /// Byte offsets inside `word` where it may break, in order.
    pub(crate) fn breaks(self, word: &str) -> impl Iterator<Item = usize> + '_ {
        let (left, right) = self.bounds();

        hypher::hyphenate_bounded(word, self.0, left, right)
            .scan(0, |offset, syllable| {
                *offset += syllable.len();
                Some(*offset)
            })
            .filter(move |&offset| offset < word.len())
    }
}
