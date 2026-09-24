#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FileTransferStatus {
    #[default]
    Selecting,
    Preparing,
    Ready,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileTransferRequest {
    EnterUsbDrive,
}

#[derive(Debug, Default)]
pub(crate) struct FileTransferState {
    status: FileTransferStatus,
    request: Option<FileTransferRequest>,
}

impl FileTransferState {
    pub(crate) const fn status(&self) -> FileTransferStatus {
        self.status
    }

    pub(crate) fn reset(&mut self) {
        self.status = FileTransferStatus::Selecting;
        self.request = None;
    }

    pub(crate) fn request_usb_drive(&mut self) -> bool {
        if self.status != FileTransferStatus::Selecting {
            return false;
        }

        self.status = FileTransferStatus::Preparing;
        self.request = Some(FileTransferRequest::EnterUsbDrive);

        true
    }

    pub(crate) fn take_request(&mut self) -> Option<FileTransferRequest> {
        self.request.take()
    }

    pub(crate) fn finish_usb_drive_request(&mut self, ready: bool) -> bool {
        if self.status != FileTransferStatus::Preparing {
            return false;
        }

        self.status = if ready {
            FileTransferStatus::Ready
        } else {
            FileTransferStatus::Error
        };

        true
    }

    pub(crate) const fn blocks_input(&self) -> bool {
        matches!(
            self.status,
            FileTransferStatus::Preparing | FileTransferStatus::Ready
        )
    }
}
