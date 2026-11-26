#![no_std]

extern crate alloc;

use alloc::{collections::BTreeMap, vec::Vec};

mod archive;
mod container;
mod cover;
mod css;
mod error;
mod image;
mod location;
mod navigation;
mod package;
mod path;
mod xhtml;
mod xml;

pub use css::{
    ChapterStyles, ComputedStyle, CssLength, FontStyle, FontWeight, LineHeight, TextAlign,
};
pub use error::{ArchiveError, ContainerError, Error, NavigationError, PackageError, XhtmlError};
pub use image::ImageDimensions;
pub use inkpaper_io::ByteSource;
pub use location::{BookLocation, ContentOffset, SpineIndex};
pub use navigation::{Navigation, NavigationEntry, NavigationTarget};
pub use package::{ManifestItem, Metadata, Package, Spine, SpineItem};
pub use path::{ArchivePath, PathError};
pub use xhtml::{
    BlockKind, Chapter, ChapterBlock, ChapterImage, Inline, LinkTarget, StyleContext, StyleNode,
    StyleNodeId, StylesheetSource, TextRun,
};

use archive::Archive;
use container::parse_container;
use css::{Stylesheet, resolve_chapter_styles};
use navigation::{parse_nav, parse_ncx};
use package::{XHTML_MEDIA_TYPE, parse_package};
use xhtml::parse_xhtml;

#[derive(Debug, Clone, Copy)]
enum NavigationFormat {
    Epub3Nav,
    Ncx,
}

pub struct Epub<S> {
    archive: Archive<S>,
    package: Package,
    stylesheet_cache: BTreeMap<ArchivePath, Stylesheet>,
}

impl<S> Epub<S>
where
    S: ByteSource,
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

        Ok(Self {
            archive,
            package,
            stylesheet_cache: BTreeMap::new(),
        })
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
            .spine_content_item(index)
            .map(|item| item.path().clone())
        else {
            return Ok(None);
        };

        let bytes = self.archive.read_entry(&path).await?;

        Ok(Some(bytes))
    }

    pub async fn spine_resource_sizes(&mut self) -> Result<Vec<Option<u64>>, Error<S::Error>> {
        let spine_len = self.package.spine().items().len();

        let mut paths = Vec::with_capacity(spine_len);
        let mut path_indices = Vec::with_capacity(spine_len);

        for index in 0..spine_len {
            match self.package.spine_content_item(index) {
                Some(item) => {
                    path_indices.push(Some(paths.len()));

                    paths.push(item.path().clone());
                }
                None => path_indices.push(None),
            }
        }

        let sizes = self
            .archive
            .uncompressed_sizes(&paths)
            .map_err(Error::Archive)?;

        Ok(path_indices
            .into_iter()
            .map(|index| index.and_then(|index| sizes[index]))
            .collect())
    }

    pub async fn load_chapter(
        &mut self,
        path: &ArchivePath,
    ) -> Result<Option<(Chapter, StyleContext)>, Error<S::Error>> {
        let Some((path, media_type)) = self.package.manifest_item_by_path(path).map(|item| {
            (
                item.path().clone(),
                alloc::string::String::from(item.media_type()),
            )
        }) else {
            return Ok(None);
        };

        if media_type != XHTML_MEDIA_TYPE {
            return Err(Error::Xhtml(XhtmlError::UnsupportedMediaType));
        }

        let bytes = self.archive.read_entry(&path).await?;
        let xml = core::str::from_utf8(&bytes).map_err(Error::Utf8)?;
        let parsed = parse_xhtml(xml, path).map_err(Error::Xhtml)?;

        Ok(Some(parsed))
    }

    pub async fn load_spine_chapter(
        &mut self,
        index: usize,
    ) -> Result<Option<(Chapter, StyleContext)>, Error<S::Error>> {
        let Some(path) = self
            .package
            .spine_content_item(index)
            .map(|item| item.path().clone())
        else {
            return Ok(None);
        };

        self.load_chapter(&path).await
    }

    /// Resolves computed styles from `style_context` and consumes it so the
    /// temporary DOM style nodes and stylesheet links are dropped immediately.
    pub async fn load_chapter_styles(
        &mut self,
        style_context: StyleContext,
    ) -> Result<ChapterStyles, Error<S::Error>> {
        let mut embedded = Vec::new();

        for source in style_context.stylesheets() {
            match source {
                StylesheetSource::Embedded(css) => {
                    embedded.push(Stylesheet::parse(css));
                }
                StylesheetSource::External(path) => {
                    let declared_css = self
                        .package
                        .manifest_item_by_path(path)
                        .is_some_and(|item| item.media_type() == "text/css");

                    if !declared_css || self.stylesheet_cache.contains_key(path) {
                        continue;
                    }

                    let bytes = self.archive.read_entry(path).await?;
                    let css = core::str::from_utf8(&bytes).map_err(Error::Utf8)?;

                    self.stylesheet_cache
                        .insert(path.clone(), Stylesheet::parse(css));
                }
            }
        }

        let mut ordered = Vec::with_capacity(style_context.stylesheets().len());
        let mut embedded_index = 0usize;

        for source in style_context.stylesheets() {
            match source {
                StylesheetSource::Embedded(_) => {
                    ordered.push(&embedded[embedded_index]);
                    embedded_index += 1;
                }
                StylesheetSource::External(path) => {
                    if let Some(sheet) = self.stylesheet_cache.get(path) {
                        ordered.push(sheet);
                    }
                }
            }
        }

        Ok(resolve_chapter_styles(&style_context, Some(&ordered)))
    }

    pub async fn image_dimensions(
        &mut self,
        path: &ArchivePath,
    ) -> Result<Option<ImageDimensions>, Error<S::Error>> {
        let Some((path, format)) = self.package.manifest_item_by_path(path).and_then(|item| {
            let format = image::ImageFormat::from_media_type(item.media_type())?;
            Some((item.path().clone(), format))
        }) else {
            return Ok(None);
        };

        let mut stream = self.archive.open_entry_stream(&path).await?;

        image::read_dimensions(format, &mut stream).await
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
