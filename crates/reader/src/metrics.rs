use inkpaper_epub::{BlockKind, ChapterImage, FontStyle, FontWeight, ImageDimensions};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextStyle {
    font_size: u16,
    block_kind: BlockKind,
    font_weight: FontWeight,
    font_style: FontStyle,
}

impl TextStyle {
    pub const fn new(
        font_size: u16,
        block_kind: BlockKind,
        font_weight: FontWeight,
        font_style: FontStyle,
    ) -> Self {
        Self {
            font_size,
            block_kind,
            font_weight,
            font_style,
        }
    }

    pub const fn font_size(self) -> u16 {
        self.font_size
    }

    pub const fn block_kind(self) -> BlockKind {
        self.block_kind
    }

    pub const fn font_weight(self) -> FontWeight {
        self.font_weight
    }

    pub const fn font_style(self) -> FontStyle {
        self.font_style
    }
}

pub trait TextMeasurer {
    type Error;

    fn measure_text(&mut self, text: &str, style: TextStyle) -> Result<u32, Self::Error>;

    fn line_height(&mut self, style: TextStyle) -> Result<u32, Self::Error>;

    fn next_boundary(
        &mut self,
        text: &str,
        from: usize,
        _style: TextStyle,
    ) -> Result<Option<usize>, Self::Error> {
        Ok(scalar_boundary(text, from))
    }
}

pub trait ImageMeasurer {
    fn image_dimensions(&mut self, _image: &ChapterImage) -> Option<ImageDimensions> {
        None
    }
}

fn scalar_boundary(text: &str, from: usize) -> Option<usize> {
    if from >= text.len() || !text.is_char_boundary(from) {
        return None;
    }

    let character = text[from..].chars().next()?;

    Some(from.saturating_add(character.len_utf8()))
}
