use alloc::{string::String, vec::Vec};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArchivePath(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathError {
    Empty,
    Absolute,
    Backslash,
    EscapesRoot,
}

impl ArchivePath {
    pub fn new(path: &str) -> Result<Self, PathError> {
        if path.is_empty() {
            return Err(PathError::Empty);
        }
        if path.starts_with('/') {
            return Err(PathError::Absolute);
        }
        if path.contains('\\') {
            return Err(PathError::Backslash);
        }

        let mut segments = Vec::new();

        for segment in path.split('/') {
            match segment {
                "" | "." => {}
                ".." => {
                    if segments.pop().is_none() {
                        return Err(PathError::EscapesRoot);
                    }
                }
                segment => segments.push(segment),
            }
        }

        if segments.is_empty() {
            return Err(PathError::Empty);
        }

        let mut normalized = String::new();

        for segment in segments {
            if !normalized.is_empty() {
                normalized.push('/');
            }

            normalized.push_str(segment);
        }

        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn resolve(&self, reference: &str) -> Result<Self, PathError> {
        let mut reference_end = reference.len();

        if let Some(index) = reference.find('#') {
            reference_end = reference_end.min(index);
        }

        if let Some(index) = reference.find('?') {
            reference_end = reference_end.min(index);
        }

        let reference = &reference[..reference_end];
        if reference.is_empty() {
            return Err(PathError::Empty);
        }

        let reference = decode_url_path(reference);
        if reference.starts_with('/') {
            return Self::new(&reference);
        }

        let parent = self
            .0
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("");

        let mut combined = String::new();

        if !parent.is_empty() {
            combined.push_str(parent);
            combined.push('/');
        }

        combined.push_str(&reference);

        Self::new(&combined)
    }
}

pub(crate) fn decode_url_component(value: &str) -> String {
    decode_percent_encoded(value, false)
}

fn decode_url_path(value: &str) -> String {
    decode_percent_encoded(value, true)
}

fn decode_percent_encoded(value: &str, preserve_separators: bool) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    let mut changed = false;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = hex_value(bytes[index + 1]);
            let low = hex_value(bytes[index + 2]);

            if let (Some(high), Some(low)) = (high, low) {
                let decoded = (high << 4) | low;

                if preserve_separators && matches!(decoded, b'/' | b'\\') {
                    output.extend_from_slice(&bytes[index..index + 3]);
                } else {
                    output.push(decoded);
                    changed = true;
                }

                index += 3;
                continue;
            }
        }

        output.push(bytes[index]);
        index += 1;
    }

    if !changed {
        return String::from(value);
    }

    String::from_utf8(output).unwrap_or_else(|_| String::from(value))
}

const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_path_normalizes_relative_segments() {
        let path = ArchivePath::new("OPS/Text/../Images/./cover.jpg").unwrap();

        assert_eq!(path.as_str(), "OPS/Images/cover.jpg",);
    }

    #[test]
    fn archive_path_resolves_relative_to_file_parent() {
        let package = ArchivePath::new("OPS/package.opf").unwrap();
        let chapter = package.resolve("Text/./chapter.xhtml").unwrap();

        assert_eq!(chapter.as_str(), "OPS/Text/chapter.xhtml",);
    }

    #[test]
    fn archive_path_rejects_escape_above_archive_root() {
        assert_eq!(
            ArchivePath::new("../../book.opf",),
            Err(PathError::EscapesRoot),
        );
    }

    #[test]
    fn archive_path_resolves_percent_encoded_url_paths() {
        let package = ArchivePath::new("OPS/package.opf").unwrap();

        let chapter = package.resolve("Text/Caf%C3%A9%20Chapter.xhtml").unwrap();

        assert_eq!(chapter.as_str(), "OPS/Text/Café Chapter.xhtml");
    }

    #[test]
    fn archive_path_normalizes_percent_encoded_dot_segments() {
        let chapter = ArchivePath::new("OPS/Text/chapter.xhtml").unwrap();

        let image = chapter.resolve("%2E%2E/Images/cover%20art.jpg").unwrap();

        assert_eq!(image.as_str(), "OPS/Images/cover art.jpg");
    }

    #[test]
    fn archive_path_rejects_percent_encoded_escape_above_root() {
        let chapter = ArchivePath::new("OPS/chapter.xhtml").unwrap();

        assert_eq!(
            chapter.resolve("%2E%2E/%2E%2E/secret.xhtml"),
            Err(PathError::EscapesRoot),
        );
    }

    #[test]
    fn archive_path_does_not_decode_encoded_separators() {
        let package = ArchivePath::new("OPS/package.opf").unwrap();

        let chapter = package.resolve("Text/part%2Fchapter.xhtml").unwrap();

        assert_eq!(chapter.as_str(), "OPS/Text/part%2Fchapter.xhtml");
    }

    #[test]
    fn archive_path_preserves_malformed_percent_escapes() {
        let package = ArchivePath::new("OPS/package.opf").unwrap();

        let chapter = package.resolve("Text/100%Ready.xhtml").unwrap();

        assert_eq!(chapter.as_str(), "OPS/Text/100%Ready.xhtml");
    }
}
