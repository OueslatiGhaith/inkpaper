#[derive(Debug)]
pub enum Error<E> {
    Source(E),
    /// an `.idx` entry runs past the end of the file, or its word past the
    /// format's 256 bytes
    MalformedIndex,
    /// an `.idx` entry points past the end of the `.dict`
    DefinitionOutOfBounds,
    /// a `.dict.dz` without dictzip's chunk table, or with one that doesn't
    /// fit the file
    MalformedDictZip,
    /// a `.dict.dz` chunk that doesn't inflate to its bytes
    Inflate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfoError {
    NotStarDict,
    Utf8,
    MissingField(&'static str),
    InvalidField(&'static str),
    /// `idxoffsetbits=64`, which only dictionaries over 4 GB need
    UnsupportedOffsetBits,
}
