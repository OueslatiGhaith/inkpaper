use inkpaper_ui::prelude::*;

use super::InkPaperApp;
use crate::{FileTransferRequest, UsbDriveConnection};

impl InkPaperApp {
    pub(crate) fn activate_usb_drive(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
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
}
