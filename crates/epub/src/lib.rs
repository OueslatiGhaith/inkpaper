#![no_std]

extern crate alloc;

mod archive;
mod container;
mod error;
mod package;
mod path;
mod source;
mod xml;

pub use error::{ArchiveError, ContainerError, Error, PackageError};

pub use package::{ManifestItem, Metadata, Package, Spine, SpineItem};

pub use path::{ArchivePath, PathError};

pub use source::{EpubSource, SliceSource, SliceSourceError};

use archive::Archive;
use container::parse_container;
use package::parse_package;

pub struct Epub<S> {
    archive: Archive<S>,
    package: Package,
}

impl<S> Epub<S>
where
    S: EpubSource,
{
    pub async fn open(source: S) -> Result<Self, Error<S::Error>> {
        let mut archive = Archive::open(source).await?;

        let container_path = ArchivePath::new("META-INF/container.xml")
            .expect("the EPUB container path is statically valid");

        let container_bytes = archive.read_entry(&container_path).await?;
        let container_xml = core::str::from_utf8(&container_bytes).map_err(Error::Utf8)?;
        let container = parse_container(container_xml).map_err(Error::Container)?;
        let package_bytes = archive.read_entry(container.package_path()).await?;
        let package_xml = core::str::from_utf8(&package_bytes).map_err(Error::Utf8)?;
        let package =
            parse_package(package_xml, container.package_path().clone()).map_err(Error::Package)?;

        Ok(Self { archive, package })
    }

    pub const fn package(&self) -> &Package {
        &self.package
    }

    pub const fn metadata(&self) -> &Metadata {
        self.package.metadata()
    }

    pub fn manifest(&self) -> &[ManifestItem] {
        self.package.manifest()
    }

    pub const fn spine(&self) -> &Spine {
        self.package.spine()
    }

    pub fn into_source(self) -> S {
        self.archive.into_source()
    }
}
