use inkpaper_ui::prelude::*;

use super::{
    InkPaperApp, Screen,
    navigation::{Back, Exit, ScreenLifecycle},
};
use crate::{FileTransferRequest, UsbDriveConnection};

impl InkPaperApp {
    pub(super) fn activate_usb_drive(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        if self.screen() != Screen::FileTransfer {
            return;
        }

        if self.file_transfer.request_usb_drive() {
            cx.notify();
        }
    }

    pub(crate) fn take_file_transfer_request(&mut self) -> Option<FileTransferRequest> {
        self.file_transfer.take_request()
    }

    pub(crate) fn apply_usb_drive_result(&mut self, ready: bool, cx: &mut Context<'_, Self>) {
        if self.file_transfer.finish_usb_drive_request(ready) {
            cx.notify();
        }
    }

    pub fn apply_usb_drive_connection(
        &mut self,
        connection: UsbDriveConnection,
        cx: &mut Context<'_, Self>,
    ) {
        if self.file_transfer.apply_usb_drive_connection(connection) {
            cx.notify();
        }
    }

    pub(super) fn file_transfer_blocks_input(&self) -> bool {
        self.screen() == Screen::FileTransfer && self.file_transfer.blocks_input()
    }
}

pub(super) struct FileTransferRoute;

impl ScreenLifecycle for FileTransferRoute {
    fn exit(&self, app: &mut InkPaperApp, exit: Exit) {
        if exit == Exit::Closed {
            app.file_transfer.reset();
        }
    }

    fn back(&self, app: &mut InkPaperApp, _: &mut Context<'_, InkPaperApp>) -> Back {
        // the SD card belongs to the host until the USB Drive session ends
        if app.file_transfer.blocks_input() {
            return Back::Handled;
        }

        Back::Leave
    }
}
