use alloc::{string::String, vec::Vec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReaderRequest {
    OpenEpub(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderDocument {
    path: String,
    title: Option<String>,
    creators: Vec<String>,
    package_path: String,
    first_spine_path: Option<String>,
    spine_len: usize,
}

impl ReaderDocument {
    pub fn new(
        path: String,
        title: Option<String>,
        creators: Vec<String>,
        package_path: String,
        first_spine_path: Option<String>,
        spine_len: usize,
    ) -> Self {
        Self {
            path,
            title,
            creators,
            package_path,
            first_spine_path,
            spine_len,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn creators(&self) -> &[String] {
        &self.creators
    }

    pub fn package_path(&self) -> &str {
        &self.package_path
    }

    pub fn first_spine_path(&self) -> Option<&str> {
        self.first_spine_path.as_deref()
    }

    pub const fn spine_len(&self) -> usize {
        self.spine_len
    }
}

#[derive(Debug, Default)]
pub(crate) struct ReaderState {
    path: String,
    fallback_title: String,
    pending: Option<ReaderRequest>,
    document: Option<ReaderDocument>,
    failed: bool,
}

impl ReaderState {
    pub(crate) fn open(&mut self, path: String, fallback_title: String) {
        self.pending = Some(ReaderRequest::OpenEpub(path.clone()));
        self.path = path;
        self.fallback_title = fallback_title;
        self.document = None;
        self.failed = false;
    }

    pub(crate) fn take_request(&mut self) -> Option<ReaderRequest> {
        self.pending.take()
    }

    pub(crate) fn apply_document(&mut self, document: ReaderDocument) -> bool {
        if document.path() != self.path {
            return false;
        }

        self.document = Some(document);
        self.failed = false;

        true
    }

    pub(crate) fn apply_error(&mut self, path: &str) -> bool {
        if path != self.path {
            return false;
        }

        self.document = None;
        self.failed = true;

        true
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn title(&self) -> &str {
        self.document
            .as_ref()
            .and_then(ReaderDocument::title)
            .unwrap_or(&self.fallback_title)
    }

    pub(crate) fn creator(&self) -> &str {
        self.document
            .as_ref()
            .and_then(|document| document.creators().first())
            .map(String::as_str)
            .unwrap_or("")
    }

    pub(crate) fn status(&self) -> &'static str {
        if self.failed {
            "Could not open EPUB"
        } else if self.document.is_some() {
            "EPUB opened"
        } else {
            "Opening EPUB..."
        }
    }

    pub(crate) fn detail(&self) -> &str {
        let Some(document) = &self.document else {
            return &self.path;
        };

        document
            .first_spine_path()
            .unwrap_or_else(|| document.package_path())
    }
}
