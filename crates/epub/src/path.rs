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
        if reference.starts_with('/') {
            return Self::new(reference);
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

        combined.push_str(reference);

        Self::new(&combined)
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
}
