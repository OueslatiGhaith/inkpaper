use alloc::{string::String, vec::Vec};
use xmlparser::{ElementEnd, Token, Tokenizer};

use crate::{
    error::PackageError,
    path::ArchivePath,
    xml::{decode_xml_value, push_decoded_xml_text},
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Metadata {
    title: Option<String>,
    creators: Vec<String>,
    language: Option<String>,
    identifier: Option<String>,
}

impl Metadata {
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn creators(&self) -> &[String] {
        &self.creators
    }

    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    pub fn identifier(&self) -> Option<&str> {
        self.identifier.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestItem {
    id: String,
    href: String,
    path: ArchivePath,
    media_type: String,
    fallback: Option<String>,
    properties: Vec<String>,
}

impl ManifestItem {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn href(&self) -> &str {
        &self.href
    }

    pub fn path(&self) -> &ArchivePath {
        &self.path
    }

    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    pub fn fallback(&self) -> Option<&str> {
        self.fallback.as_deref()
    }

    pub fn properties(&self) -> &[String] {
        &self.properties
    }

    pub fn has_property(&self, property: &str) -> bool {
        self.properties
            .iter()
            .any(|candidate| candidate == property)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineItem {
    idref: String,
    linear: bool,
}

impl SpineItem {
    pub fn idref(&self) -> &str {
        &self.idref
    }

    pub const fn linear(&self) -> bool {
        self.linear
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Spine {
    toc: Option<String>,
    items: Vec<SpineItem>,
}

impl Spine {
    pub fn toc(&self) -> Option<&str> {
        self.toc.as_deref()
    }

    pub fn items(&self) -> &[SpineItem] {
        &self.items
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    path: ArchivePath,
    version: Option<String>,
    unique_identifier: Option<String>,
    metadata: Metadata,
    manifest: Vec<ManifestItem>,
    spine: Spine,
}

impl Package {
    pub fn path(&self) -> &ArchivePath {
        &self.path
    }

    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    pub fn unique_identifier(&self) -> Option<&str> {
        self.unique_identifier.as_deref()
    }

    pub const fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    pub fn manifest(&self) -> &[ManifestItem] {
        &self.manifest
    }

    pub const fn spine(&self) -> &Spine {
        &self.spine
    }

    pub fn manifest_item(&self, id: &str) -> Option<&ManifestItem> {
        self.manifest.iter().find(|item| item.id == id)
    }

    pub fn manifest_item_by_path(&self, path: &ArchivePath) -> Option<&ManifestItem> {
        self.manifest.iter().find(|item| item.path() == path)
    }

    pub fn spine_manifest_item(&self, index: usize) -> Option<&ManifestItem> {
        let spine = self.spine.items.get(index)?;

        self.manifest_item(spine.idref())
    }

    /// Returns the item a reading system should render for a spine entry.
    ///
    /// Follows the manifest `fallback` chain until it reaches an XHTML content
    /// document. If the chain ends, is broken, or cycles without finding one,
    /// the spine's own item is returned.
    pub fn spine_content_item(&self, index: usize) -> Option<&ManifestItem> {
        let item = self.spine_manifest_item(index)?;

        let mut current = item;

        // a chain can visit each manifest item at most once, which bounds cycles
        for _ in 0..self.manifest.len() {
            if current.media_type == XHTML_MEDIA_TYPE {
                return Some(current);
            }

            let Some(next) = current.fallback().and_then(|id| self.manifest_item(id)) else {
                break;
            };

            current = next;
        }

        Some(item)
    }
}

pub(crate) const XHTML_MEDIA_TYPE: &str = "application/xhtml+xml";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    Metadata,
    Manifest,
    Spine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetadataField {
    Title,
    Creator,
    Language,
    Identifier,
}

impl MetadataField {
    fn from_local_name(name: &str) -> Option<Self> {
        match name {
            "title" => Some(Self::Title),
            "creator" => Some(Self::Creator),
            "language" => Some(Self::Language),
            "identifier" => Some(Self::Identifier),
            _ => None,
        }
    }

    const fn local_name(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Creator => "creator",
            Self::Language => "language",
            Self::Identifier => "identifier",
        }
    }
}

struct TextCapture {
    field: MetadataField,
    value: String,
}

#[derive(Default)]
struct PendingManifestItem {
    id: Option<String>,
    href: Option<String>,
    media_type: Option<String>,
    fallback: Option<String>,
    properties: Option<String>,
}

impl PendingManifestItem {
    fn finish(self, package_path: &ArchivePath) -> Result<ManifestItem, PackageError> {
        let id = self
            .id
            .ok_or(PackageError::MissingManifestAttribute("id"))?;

        let href = self
            .href
            .ok_or(PackageError::MissingManifestAttribute("href"))?;

        let media_type = self
            .media_type
            .ok_or(PackageError::MissingManifestAttribute("media-type"))?;

        let path = package_path.resolve(&href).map_err(PackageError::Path)?;

        let properties = self
            .properties
            .as_deref()
            .unwrap_or("")
            .split_ascii_whitespace()
            .map(String::from)
            .collect();

        Ok(ManifestItem {
            id,
            href,
            path,
            media_type,
            fallback: self.fallback,
            properties,
        })
    }
}

#[derive(Default)]
struct PendingSpineItem {
    idref: Option<String>,
    linear: Option<String>,
}

impl PendingSpineItem {
    fn finish(self) -> Result<SpineItem, PackageError> {
        let idref = self.idref.ok_or(PackageError::MissingSpineIdref)?;

        let linear = match self.linear.as_deref() {
            None | Some("yes") => true,
            Some("no") => false,
            Some(_) => return Err(PackageError::InvalidSpineLinear),
        };

        Ok(SpineItem { idref, linear })
    }
}

enum StartTag {
    Other,
    Package,
    ManifestItem(PendingManifestItem),
    Spine { toc: Option<String> },
    SpineItem(PendingSpineItem),
    MetadataField(MetadataField),
}

struct PackageParser {
    path: ArchivePath,

    version: Option<String>,
    unique_identifier: Option<String>,

    metadata: Metadata,
    manifest: Vec<ManifestItem>,

    spine_toc: Option<String>,
    spine_items: Vec<SpineItem>,

    section: Section,
    start: StartTag,
    capture: Option<TextCapture>,
}

impl PackageParser {
    fn new(path: ArchivePath) -> Self {
        Self {
            path,
            version: None,
            unique_identifier: None,
            metadata: Metadata::default(),
            manifest: Vec::new(),
            spine_toc: None,
            spine_items: Vec::new(),
            section: Section::None,
            start: StartTag::Other,
            capture: None,
        }
    }

    fn element_start(&mut self, name: &str) {
        self.start = match name {
            "package" => StartTag::Package,
            "metadata" => {
                self.section = Section::Metadata;
                StartTag::Other
            }
            "manifest" => {
                self.section = Section::Manifest;
                StartTag::Other
            }
            "spine" => {
                self.section = Section::Spine;
                StartTag::Spine { toc: None }
            }
            "item" if self.section == Section::Manifest => {
                StartTag::ManifestItem(PendingManifestItem::default())
            }
            "itemref" if self.section == Section::Spine => {
                StartTag::SpineItem(PendingSpineItem::default())
            }
            name if self.section == Section::Metadata => {
                match MetadataField::from_local_name(name) {
                    Some(field) => {
                        if self.capture.is_none() {
                            self.capture = Some(TextCapture {
                                field,
                                value: String::new(),
                            });
                        }

                        StartTag::MetadataField(field)
                    }

                    None => StartTag::Other,
                }
            }
            _ => StartTag::Other,
        };
    }

    fn attribute(&mut self, name: &str, value: &str) {
        match &mut self.start {
            StartTag::Package => match name {
                "version" => self.version = Some(decode_xml_value(value)),
                "unique-identifier" => self.unique_identifier = Some(decode_xml_value(value)),
                _ => {}
            },
            StartTag::ManifestItem(item) => match name {
                "id" => item.id = Some(decode_xml_value(value)),
                "href" => item.href = Some(decode_xml_value(value)),
                "media-type" => item.media_type = Some(decode_xml_value(value)),
                "fallback" => item.fallback = Some(decode_xml_value(value)),
                "properties" => item.properties = Some(decode_xml_value(value)),
                _ => {}
            },
            StartTag::Spine { toc } => {
                if name == "toc" {
                    *toc = Some(decode_xml_value(value));
                }
            }
            StartTag::SpineItem(item) => match name {
                "idref" => item.idref = Some(decode_xml_value(value)),
                "linear" => item.linear = Some(decode_xml_value(value)),
                _ => {}
            },

            StartTag::Other | StartTag::MetadataField(_) => {}
        }
    }

    fn element_boundary(&mut self, empty: bool) -> Result<(), PackageError> {
        let start = core::mem::replace(&mut self.start, StartTag::Other);

        match start {
            StartTag::ManifestItem(item) => self.manifest.push(item.finish(&self.path)?),
            StartTag::Spine { toc } => self.spine_toc = toc,
            StartTag::SpineItem(item) => self.spine_items.push(item.finish()?),
            StartTag::MetadataField(field) if empty => self.finish_capture(field),
            _ => {}
        }

        Ok(())
    }

    fn close_element(&mut self, name: &str) {
        if let Some(field) = MetadataField::from_local_name(name) {
            self.finish_capture(field);
        }

        match name {
            "metadata" | "manifest" | "spine" => self.section = Section::None,
            _ => {}
        }
    }

    fn text(&mut self, text: &str, cdata: bool) {
        let Some(capture) = &mut self.capture else {
            return;
        };

        if cdata {
            capture.value.push_str(text);
        } else {
            push_decoded_xml_text(&mut capture.value, text);
        }
    }

    fn finish_capture(&mut self, field: MetadataField) {
        let Some(capture) = self.capture.take() else {
            return;
        };

        if capture.field != field {
            self.capture = Some(capture);
            return;
        }

        let value = capture.value.trim();
        if value.is_empty() {
            return;
        }

        let value = String::from(value);

        match field {
            MetadataField::Title => {
                if self.metadata.title.is_none() {
                    self.metadata.title = Some(value);
                }
            }
            MetadataField::Creator => self.metadata.creators.push(value),
            MetadataField::Language => {
                if self.metadata.language.is_none() {
                    self.metadata.language = Some(value);
                }
            }
            MetadataField::Identifier => {
                if self.metadata.identifier.is_none() {
                    self.metadata.identifier = Some(value);
                }
            }
        }
    }

    fn finish(self) -> Package {
        Package {
            path: self.path,
            version: self.version,
            unique_identifier: self.unique_identifier,
            metadata: self.metadata,
            manifest: self.manifest,

            spine: Spine {
                toc: self.spine_toc,
                items: self.spine_items,
            },
        }
    }
}

pub(crate) fn parse_package(xml: &str, path: ArchivePath) -> Result<Package, PackageError> {
    let mut parser = PackageParser::new(path);

    for token in Tokenizer::from(xml) {
        match token.map_err(PackageError::Xml)? {
            Token::ElementStart { local, .. } => {
                parser.element_start(local.as_str());
            }
            Token::Attribute { local, value, .. } => {
                parser.attribute(local.as_str(), value.as_str());
            }
            Token::Text { text } => {
                parser.text(text.as_str(), false);
            }
            Token::Cdata { text, .. } => {
                parser.text(text.as_str(), true);
            }
            Token::ElementEnd {
                end: ElementEnd::Open,
                ..
            } => {
                parser.element_boundary(false)?;
            }
            Token::ElementEnd {
                end: ElementEnd::Empty,
                ..
            } => {
                parser.element_boundary(true)?;
            }
            Token::ElementEnd {
                end: ElementEnd::Close(_, local),
                ..
            } => {
                parser.close_element(local.as_str());
            }
            _ => {}
        }
    }

    Ok(parser.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(manifest: &str) -> Package {
        let xml = alloc::format!(
            r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
                <manifest>{manifest}</manifest>
                <spine><itemref idref="start"/></spine>
            </package>"#
        );

        parse_package(&xml, ArchivePath::new("OEBPS/content.opf").unwrap()).unwrap()
    }

    fn content_id(package: &Package) -> &str {
        package.spine_content_item(0).unwrap().id()
    }

    #[test]
    fn spine_content_item_follows_multi_step_fallback_chain() {
        let package = package(
            r#"<item id="start" href="a.txt" media-type="text/plain" fallback="middle"/>
               <item id="middle" href="b.svg" media-type="image/svg+xml" fallback="end"/>
               <item id="end" href="c.xhtml" media-type="application/xhtml+xml"/>"#,
        );

        assert_eq!(content_id(&package), "end");
        assert_eq!(package.spine_manifest_item(0).unwrap().id(), "start");
    }

    #[test]
    fn spine_content_item_keeps_xhtml_items_without_following_fallback() {
        let package = package(
            r#"<item id="start" href="a.xhtml" media-type="application/xhtml+xml" fallback="other"/>
               <item id="other" href="b.xhtml" media-type="application/xhtml+xml"/>"#,
        );

        assert_eq!(content_id(&package), "start");
    }

    #[test]
    fn spine_content_item_returns_spine_item_for_broken_or_cyclic_chains() {
        let broken = package(
            r#"<item id="start" href="a.txt" media-type="text/plain" fallback="missing"/>"#,
        );
        let cyclic = package(
            r#"<item id="start" href="a.txt" media-type="text/plain" fallback="loop"/>
               <item id="loop" href="b.txt" media-type="text/plain" fallback="start"/>"#,
        );

        assert_eq!(content_id(&broken), "start");
        assert_eq!(content_id(&cyclic), "start");
    }
}
