#![no_std]

extern crate alloc;

use alloc::vec::Vec;

mod archive;
mod container;
mod error;
mod navigation;
mod package;
mod path;
mod source;
mod xhtml;
mod xml;

pub use error::{ArchiveError, ContainerError, Error, NavigationError, PackageError, XhtmlError};
pub use navigation::{Navigation, NavigationEntry, NavigationTarget};
pub use package::{ManifestItem, Metadata, Package, Spine, SpineItem};
pub use path::{ArchivePath, PathError};
pub use source::{EpubSource, SliceSource, SliceSourceError};
pub use xhtml::{
    BlockKind, Chapter, ChapterBlock, Inline, InlineStyle, LinkTarget, StyleNode, StyleNodeId,
    StylesheetSource, TextRun,
};

use archive::Archive;
use container::parse_container;
use navigation::{parse_nav, parse_ncx};
use package::parse_package;

use crate::xhtml::parse_xhtml;

#[derive(Debug, Clone, Copy)]
enum NavigationFormat {
    Epub3Nav,
    Ncx,
}

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

    pub async fn load_navigation(&mut self) -> Result<Option<Navigation>, Error<S::Error>> {
        let Some((path, format)) = navigation_resource(&self.package) else {
            return Ok(None);
        };

        let bytes = self.archive.read_entry(&path).await?;
        let xml = core::str::from_utf8(&bytes).map_err(Error::Utf8)?;

        let navigation = match format {
            NavigationFormat::Epub3Nav => parse_nav(xml, path),
            NavigationFormat::Ncx => parse_ncx(xml, path),
        }
        .map_err(Error::Navigation)?;

        Ok(Some(navigation))
    }

    pub async fn read_resource(
        &mut self,
        path: &ArchivePath,
    ) -> Result<Option<Vec<u8>>, Error<S::Error>> {
        if self.package.manifest_item_by_path(path).is_none() {
            return Ok(None);
        }

        let bytes = self.archive.read_entry(path).await?;

        Ok(Some(bytes))
    }

    pub async fn read_manifest_resource(
        &mut self,
        id: &str,
    ) -> Result<Option<Vec<u8>>, Error<S::Error>> {
        let Some(path) = self
            .package
            .manifest_item(id)
            .map(|item| item.path().clone())
        else {
            return Ok(None);
        };

        let bytes = self.archive.read_entry(&path).await?;

        Ok(Some(bytes))
    }

    pub async fn read_spine_resource(
        &mut self,
        index: usize,
    ) -> Result<Option<Vec<u8>>, Error<S::Error>> {
        let Some(path) = self
            .package
            .spine_manifest_item(index)
            .map(|item| item.path().clone())
        else {
            return Ok(None);
        };

        let bytes = self.archive.read_entry(&path).await?;

        Ok(Some(bytes))
    }

    pub fn into_source(self) -> S {
        self.archive.into_source()
    }

    pub async fn load_chapter(
        &mut self,
        path: &ArchivePath,
    ) -> Result<Option<Chapter>, Error<S::Error>> {
        let Some((path, media_type)) = self.package.manifest_item_by_path(path).map(|item| {
            (
                item.path().clone(),
                alloc::string::String::from(item.media_type()),
            )
        }) else {
            return Ok(None);
        };

        if media_type != "application/xhtml+xml" {
            return Err(Error::Xhtml(XhtmlError::UnsupportedMediaType));
        }

        let bytes = self.archive.read_entry(&path).await?;
        let xml = core::str::from_utf8(&bytes).map_err(Error::Utf8)?;
        let chapter = parse_xhtml(xml, path).map_err(Error::Xhtml)?;

        Ok(Some(chapter))
    }

    pub async fn load_spine_chapter(
        &mut self,
        index: usize,
    ) -> Result<Option<Chapter>, Error<S::Error>> {
        let Some(path) = self
            .package
            .spine_manifest_item(index)
            .map(|item| item.path().clone())
        else {
            return Ok(None);
        };

        self.load_chapter(&path).await
    }
}

fn navigation_resource(package: &Package) -> Option<(ArchivePath, NavigationFormat)> {
    if let Some(item) = package
        .manifest()
        .iter()
        .find(|item| item.has_property("nav"))
    {
        return Some((item.path().clone(), NavigationFormat::Epub3Nav));
    }

    if let Some(toc_id) = package.spine().toc()
        && let Some(item) = package.manifest_item(toc_id)
    {
        return Some((item.path().clone(), NavigationFormat::Ncx));
    }

    package
        .manifest()
        .iter()
        .find(|item| item.media_type() == "application/x-dtbncx+xml")
        .map(|item| (item.path().clone(), NavigationFormat::Ncx))
}
