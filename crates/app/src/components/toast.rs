use inkpaper_ui::prelude::*;

/// Which toast is shown, so dismissing one never hides another shown since.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToastId {
    OpeningBook,
}

/// A short message over the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Toast {
    id: ToastId,
    message: &'static str,
}

impl Toast {
    pub(crate) const fn new(id: ToastId, message: &'static str) -> Self {
        Self { id, message }
    }
}

/// The toast over every screen, one at a time like crosspoint's popup: a
/// newer toast replaces the one shown. Its own entity, so showing or
/// dismissing one repaints only the toast.
#[derive(Debug, Default)]
pub(crate) struct ToastLayer {
    active: Option<Toast>,
}

impl ToastLayer {
    pub(crate) fn show(&mut self, toast: Toast, cx: &mut Context<'_, Self>) {
        if self.active != Some(toast) {
            self.active = Some(toast);
            cx.notify();
        }
    }

    /// Dismisses toast `id`, if it's the one shown.
    pub(crate) fn dismiss(&mut self, id: ToastId, cx: &mut Context<'_, Self>) {
        if self.active.is_some_and(|active| active.id == id) {
            self.active = None;
            cx.notify();
        }
    }
}

impl Render for ToastLayer {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let message = self.active.map(|toast| toast.message);

        // crosspoint's Lyra popup: a black box a sixth of the way down, its
        // white ring keeping it clear of black under it
        rsx! {
            <div class="absolute left-0 top-[132px] w-[480px] flex justify-center">
                {#if let Some(message) = message}
                    <div class="rounded-md border-2 border-white bg-black px-4 py-3">
                        <text class="text-xl text-white no-wrap max-lines-1">
                            {message}
                        </text>
                    </div>
                {/if}
            </div>
        }
    }
}
