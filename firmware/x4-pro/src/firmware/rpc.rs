//! Request/reply calls into a service task over a command channel.

use alloc::sync::Arc;

use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal,
};

/// The one-shot reply slot a command carries back to its caller.
pub(crate) struct Reply<T>(Arc<Signal<CriticalSectionRawMutex, T>>);

impl<T: Send> Reply<T> {
    pub(crate) fn send(self, value: T) {
        self.0.signal(value);
    }
}

impl<T> core::fmt::Debug for Reply<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Reply")
    }
}

/// Sends the command `command` builds around a fresh reply and waits for the answer.
pub(crate) async fn call<C, T: Send, const N: usize>(
    commands: &Channel<CriticalSectionRawMutex, C, N>,
    command: impl FnOnce(Reply<T>) -> C,
) -> T {
    let reply = Arc::new(Signal::new());
    commands.send(command(Reply(reply.clone()))).await;
    reply.wait().await
}
