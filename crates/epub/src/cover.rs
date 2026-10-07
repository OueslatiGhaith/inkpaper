use xmlparser::{Token, Tokenizer};

use crate::{
    ArchivePath, Epub, EpubSource, Error, package::XHTML_MEDIA_TYPE, xml::decode_xml_value,
};

impl<S> Epub<S>
where
    S: EpubSource,
{
    /// The path of the book's cover image, like crosspoint finds it: the first
    /// of [`crate::Package::cover_candidates`] that is an image, or a page
    /// showing one. `None` when the book declares no cover.
    pub async fn cover_image(&mut self) -> Result<Option<ArchivePath>, Error<S::Error>> {
        let candidates: alloc::vec::Vec<_> = self
            .package
            .cover_candidates()
            .map(|item| (item.path().clone(), item.media_type() == XHTML_MEDIA_TYPE))
            .collect();

        for (path, page) in candidates {
            if !page {
                return Ok(Some(path));
            }

            let bytes = self.archive.read_entry(&path).await?;

            let Ok(xhtml) = core::str::from_utf8(&bytes) else {
                continue;
            };

            let image = first_image(xhtml, &path)
                .filter(|image| self.package.manifest_item_by_path(image).is_some());

            if image.is_some() {
                return Ok(image);
            }
        }

        Ok(None)
    }
}

/// The first image a cover page shows: an `<img>`, or an SVG `<image>` as
/// cover pages often wrap theirs.
fn first_image(xhtml: &str, page: &ArchivePath) -> Option<ArchivePath> {
    let mut element = "";

    for token in Tokenizer::from(xhtml) {
        match token.ok()? {
            Token::ElementStart { local, .. } => element = local.as_str(),

            Token::Attribute { local, value, .. } => {
                let source = matches!(
                    (element, local.as_str()),
                    ("img", "src") | ("image", "href")
                );

                if source {
                    return page.resolve(&decode_xml_value(value.as_str())).ok();
                }
            }

            _ => {}
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> ArchivePath {
        ArchivePath::new("OEBPS/Text/cover.xhtml").unwrap()
    }

    #[test]
    fn cover_pages_show_an_img_or_an_svg_image() {
        let img = r#"<html><body><div><img alt="" src="../Images/cover.jpg"/></div></body></html>"#;

        assert_eq!(
            first_image(img, &page()),
            ArchivePath::new("OEBPS/Images/cover.jpg").ok(),
        );

        let svg = r#"<html><body><svg xmlns:xlink="http://www.w3.org/1999/xlink">
            <image width="600" height="900" xlink:href="../Images/cover%20art.png"/>
        </svg></body></html>"#;

        assert_eq!(
            first_image(svg, &page()),
            ArchivePath::new("OEBPS/Images/cover art.png").ok(),
        );

        assert_eq!(
            first_image("<html><body><p>No image</p></body></html>", &page()),
            None
        );
    }
}
