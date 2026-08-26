pub const BOOKS_DIR: &str = "/Books";
pub const APP_DIR: &str = "/InkPaper";
pub const SETTINGS_FILE: &str = "/InkPaper/settings.cbor";
pub const BOOK_STATE_DIR: &str = "/InkPaper/books";

pub fn is_book_path(path: &str) -> bool {
    let Some(relative) = path.strip_prefix(BOOKS_DIR) else {
        return false;
    };

    relative.starts_with('/') && relative.len() > 1
}
