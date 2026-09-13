use crate::{Element, MountCx, MountError, NodeId, Pixels, Size, SvgSource, TextStyle, TextStyled};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SvgStyle {
    pub(crate) width: Option<Pixels>,
    pub(crate) height: Option<Pixels>,
}

#[derive(Debug, Clone, Copy)]
pub struct Svg {
    source: SvgSource,
    style: SvgStyle,
    text_style: TextStyle,
}

impl Svg {
    pub const fn new(source: SvgSource) -> Self {
        Self {
            source,
            style: SvgStyle {
                width: None,
                height: None,
            },
            text_style: TextStyle::DEFAULT,
        }
    }

    pub const fn source(self) -> SvgSource {
        self.source
    }

    pub fn w(mut self, width: Pixels) -> Self {
        self.style.width = Some(width);
        self
    }

    pub fn h(mut self, height: Pixels) -> Self {
        self.style.height = Some(height);
        self
    }

    pub fn size(mut self, size: Size) -> Self {
        self.style.width = Some(size.width);
        self.style.height = Some(size.height);
        self
    }
}

impl TextStyled for Svg {
    fn text_style_mut(&mut self) -> &mut TextStyle {
        &mut self.text_style
    }
}

impl Element for Svg {
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        cx.push_svg(self.source, self.style, self.text_style)
    }
}

pub const fn svg(source: SvgSource) -> Svg {
    Svg::new(source)
}
