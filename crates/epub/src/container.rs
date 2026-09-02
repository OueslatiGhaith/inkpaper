use xmlparser::{Token, Tokenizer};

use crate::{error::ContainerError, path::ArchivePath, xml::decode_xml_value};

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
                    let package_path = ArchivePath::new(&path).map_err(ContainerError::Path)?;

                    return Ok(Container { package_path });
                }

                in_rootfile = false;
            }

            _ => {}
        }
    }

    Err(ContainerError::MissingRootfile)
}
