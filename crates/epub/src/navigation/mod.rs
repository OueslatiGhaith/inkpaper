use alloc::{string::String, vec::Vec};

use crate::{ArchivePath, PathError, path::decode_url_component};

mod nav;
mod ncx;

pub(crate) use nav::parse_nav;
pub(crate) use ncx::parse_ncx;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Navigation {
    entries: Vec<NavigationEntry>,
}

impl Navigation {
    pub fn entries(&self) -> &[NavigationEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavigationEntry {
    label: String,
    target: Option<NavigationTarget>,
    children: Vec<NavigationEntry>,
}

impl NavigationEntry {
    pub fn label(&self) -> &str {
        &self.label
    }

    pub const fn target(&self) -> Option<&NavigationTarget> {
        self.target.as_ref()
    }

    pub fn children(&self) -> &[NavigationEntry] {
        &self.children
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavigationTarget {
    path: ArchivePath,
    fragment: Option<String>,
}

impl NavigationTarget {
    pub fn path(&self) -> &ArchivePath {
        &self.path
    }

    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_deref()
    }

    fn resolve(base: &ArchivePath, reference: &str) -> Result<Self, PathError> {
        let (resource_reference, fragment) = match reference.split_once('#') {
            Some((resource, fragment)) => (
                resource,
                if fragment.is_empty() {
                    None
                } else {
                    Some(decode_url_component(fragment))
                },
            ),

            None => (reference, None),
        };

        let resource_reference = resource_reference
            .split_once('?')
            .map(|(resource, _)| resource)
            .unwrap_or(resource_reference);

        let path = if resource_reference.is_empty() {
            base.clone()
        } else {
            base.resolve(resource_reference)?
        };

        Ok(Self { path, fragment })
    }
}

#[derive(Default)]
struct EntryBuilder {
    label: Option<String>,
    target: Option<NavigationTarget>,
    children: Vec<NavigationEntry>,
}

impl EntryBuilder {
    fn set_label(&mut self, label: String) {
        if self.label.is_some() {
            return;
        }

        let label = normalize_label(&label);

        if !label.is_empty() {
            self.label = Some(label);
        }
    }

    fn set_target(&mut self, target: NavigationTarget) {
        if self.target.is_none() {
            self.target = Some(target);
        }
    }
}

fn close_entry(stack: &mut Vec<EntryBuilder>, roots: &mut Vec<NavigationEntry>) {
    let Some(mut builder) = stack.pop() else {
        return;
    };

    let Some(label) = builder.label.take() else {
        // be forgiving of malformed wrapper entries: preserve any valid nested entries.
        if let Some(parent) = stack.last_mut() {
            parent.children.extend(builder.children);
        } else {
            roots.extend(builder.children);
        }

        return;
    };

    let entry = NavigationEntry {
        label,
        target: builder.target,
        children: builder.children,
    };

    if let Some(parent) = stack.last_mut() {
        parent.children.push(entry);
    } else {
        roots.push(entry);
    }
}

fn normalize_label(input: &str) -> String {
    let mut output = String::new();

    let mut pending_space = false;

    for character in input.chars() {
        if character.is_whitespace() {
            if !output.is_empty() {
                pending_space = true;
            }

            continue;
        }

        if pending_space {
            output.push(' ');
            pending_space = false;
        }

        output.push(character);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epub3_navigation_decodes_percent_encoded_targets() {
        const NAV: &str = r#"
<html
    xmlns="http://www.w3.org/1999/xhtml"
    xmlns:epub="http://www.idpf.org/2007/ops"
>
    <body>
        <nav epub:type="toc">
            <ol>
                <li>
                    <a href="../Text/Caf%C3%A9%20Chapter.xhtml#section%201">
                        Chapter One
                    </a>
                </li>
            </ol>
        </nav>
    </body>
</html>
"#;

        let navigation =
            parse_nav(NAV, ArchivePath::new("OPS/Navigation/toc.xhtml").unwrap()).unwrap();

        assert_eq!(navigation.entries().len(), 1);

        let target = navigation.entries()[0].target().unwrap();

        assert_eq!(target.path().as_str(), "OPS/Text/Café Chapter.xhtml");
        assert_eq!(target.fragment(), Some("section 1"));
    }

    #[test]
    fn epub2_ncx_decodes_percent_encoded_targets() {
        const NCX: &str = r#"
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/">
    <navMap>
        <navPoint id="chapter-1">
            <navLabel>
                <text>Chapter One</text>
            </navLabel>

            <content src="../Text/Caf%C3%A9%20Chapter.xhtml#section%201"/>
        </navPoint>
    </navMap>
</ncx>
"#;

        let navigation =
            parse_ncx(NCX, ArchivePath::new("OPS/Navigation/toc.ncx").unwrap()).unwrap();

        assert_eq!(navigation.entries().len(), 1);

        let target = navigation.entries()[0].target().unwrap();

        assert_eq!(target.path().as_str(), "OPS/Text/Café Chapter.xhtml");
        assert_eq!(target.fragment(), Some("section 1"));
    }
}
