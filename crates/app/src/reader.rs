use alloc::string::String;

#[derive(Debug, Default)]
pub(crate) struct ReaderState {
    path: String,
    title: String,
}

impl ReaderState {
    pub(crate) fn open(&mut self, path: String, title: String) {
        self.path = path;
        self.title = title;
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }
}
