use xmlparser::{Token, Tokenizer};

use crate::{
    error::ContainerError,
    path::{ArchivePath, decode_url_path},
    xml::decode_xml_value,
};

pub(crate) struct Container {
    package_path: ArchivePath,
}

impl Container {
    pub(crate) fn package_path(&self) -> &ArchivePath {
        &self.package_path
    }
}

pub(crate) fn parse_container(xml: &str) -> Result<Container, ContainerError> {
    let mut in_rootfile = false;
    let mut full_path = None;

    for token in Tokenizer::from(xml) {
        match token.map_err(ContainerError::Xml)? {
            Token::ElementStart { local, .. } => {
                in_rootfile = local.as_str() == "rootfile";

                if in_rootfile {
                    full_path = None;
                }
            }

            Token::Attribute { local, value, .. }
                if in_rootfile && local.as_str() == "full-path" =>
            {
                full_path = Some(decode_xml_value(value.as_str()));
            }

            Token::ElementEnd { .. } if in_rootfile => {
                if let Some(path) = full_path.take() {
                    // full-path is specified as a plain path, but some producers
                    // percent-encode it like a URL
                    let package_path =
                        ArchivePath::new(&decode_url_path(&path)).map_err(ContainerError::Path)?;

                    return Ok(Container { package_path });
                }

                in_rootfile = false;
            }

            _ => {}
        }
    }

    Err(ContainerError::MissingRootfile)
}

#[cfg(test)]
mod tests {
    use alloc::{format, string::String};

    use super::*;
    use crate::path::PathError;

    fn container_xml(full_path: &str) -> String {
        format!(
            r#"<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
                <rootfiles>
                    <rootfile full-path="{full_path}" media-type="application/oebps-package+xml"/>
                </rootfiles>
            </container>"#
        )
    }

    #[test]
    fn container_decodes_percent_encoded_rootfile_path() {
        let container = parse_container(&container_xml("OEBPS/My%20Book/content.opf")).unwrap();

        assert_eq!(
            container.package_path().as_str(),
            "OEBPS/My Book/content.opf"
        );
    }

    #[test]
    fn container_keeps_encoded_separators_in_rootfile_path() {
        let container = parse_container(&container_xml("OEBPS/a%2Fb/content.opf")).unwrap();

        assert_eq!(container.package_path().as_str(), "OEBPS/a%2Fb/content.opf");
    }

    #[test]
    fn container_rejects_percent_encoded_escape_above_root() {
        let result = parse_container(&container_xml("%2E%2E/content.opf"));

        assert!(matches!(
            result,
            Err(ContainerError::Path(PathError::EscapesRoot))
        ));
    }
}
