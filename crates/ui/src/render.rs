use crate::{Context, IntoElement};

pub trait Render: Sized + 'static {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a;
}
