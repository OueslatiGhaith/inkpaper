use core::str::Utf8Error;

use crate::PathError;

#[derive(Debug)]
pub enum Error<E> {
    Source(E),
    Archive(ArchiveError),
    Utf8(Utf8Error),
    Container(ContainerError),
    Package(PackageError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveError {
    EndOfCentralDirectoryMissing,
    InvalidCentralDirectory,
    InvalidLocalHeader,
    MultiDiskUnsupported,
    Zip64Unsupported,
    EncryptedEntry,
    EntryNotFound,
    UnsupportedCompression(u16),
    Inflate,
    SizeOverflow,
    SizeMismatch { expected: u32, actual: usize },
}

#[derive(Debug)]
pub enum ContainerError {
    Xml(xmlparser::Error),
    MissingRootfile,
    Path(PathError),
}

#[derive(Debug)]
pub enum PackageError {
    Xml(xmlparser::Error),
    Path(PathError),
    MissingManifestAttribute(&'static str),
    MissingSpineIdref,
    InvalidSpineLinear,
}
