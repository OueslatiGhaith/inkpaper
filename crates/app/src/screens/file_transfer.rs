use inkpaper_ui::prelude::*;

use crate::{
    BatteryStatus, FileTransferStatus, InkPaperApp,
    app::{Back, Exit, ScreenInput, ScreenLifecycle, ScreenView},
    components::{
        header::{BackHeader, BackHeaderProps, TitleHeader, TitleHeaderProps},
        icon::IconKind,
        transfer_mode_row::{TransferModeRow, TransferModeRowProps},
    },
};

#[component]
pub(crate) struct FileTransferScreen {
    status: FileTransferStatus,
    battery: Option<BatteryStatus>,
    on_back: Listener<ActivateEvent>,
    on_usb_drive: Listener<ActivateEvent>,
}

impl RenderOnce for FileTransferScreen {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let selecting = self.status == FileTransferStatus::Selecting;
        let error = self.status == FileTransferStatus::Error;

        let (message, detail, secondary_detail) = match self.status {
            FileTransferStatus::Selecting => ("", None, None),

            FileTransferStatus::Preparing => (
                "Preparing USB Drive...",
                Some("Eject the drive on your computer, or disconnect the cable to return Home."),
                None,
            ),

            FileTransferStatus::WaitingForHost => {
                ("Connect this reader to your computer", None, None)
            }

            FileTransferStatus::Connected => (
                "USB Drive Connected",
                Some("This may take up to 30 seconds to connect"),
                Some("Eject the drive on your computer, or disconnect the cable to return Home."),
            ),

            FileTransferStatus::Error => (
                "Unable to start USB Drive.",
                Some("Press Back to return Home."),
                None,
            ),
        };

        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                {#if selecting}
                    <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                        <BackHeader
                            title="File Transfer"
                            battery={self.battery}
                            on_back={self.on_back}
                        />
                    </div>

                    <div class="absolute left-0 top-[98px] w-[480px]">
                        <TransferModeRow
                            id="file-transfer-usb-drive"
                            title="USB Drive"
                            subtitle="Manage the SD card from your computer"
                            icon={IconKind::Usb}
                            on_activate={self.on_usb_drive}
                        />
                    </div>
                {:else}
                    <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                        {#if error}
                            <BackHeader
                                title="USB Drive"
                                battery={self.battery}
                                on_back={self.on_back}
                            />
                        {:else}
                            <TitleHeader
                                title="USB Drive"
                                battery={self.battery}
                            />
                        {/if}
                    </div>

                    <div class="absolute left-5 top-20 w-[440px] h-[700px] flex flex-col items-center justify-center gap-4">
                        <text class="font-bold text-xl text-center wrap max-lines-2">
                            {message}
                        </text>

                        {#if let Some(detail) = detail}
                            <text class="text-base text-center wrap max-lines-3">
                                {detail}
                            </text>
                        {/if}

                        {#if let Some(detail) = secondary_detail}
                            <text class="text-base text-center wrap max-lines-3">
                                {detail}
                            </text>
                        {/if}
                    </div>
                {/if}
            </div>
        }
    }
}

pub(crate) struct FileTransferRoute;

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

impl ScreenInput for FileTransferRoute {
    fn blocks_input(&self, app: &InkPaperApp) -> bool {
        app.file_transfer.blocks_input()
    }
}

impl ScreenView for FileTransferRoute {
    fn render<'a>(
        &self,
        app: &'a InkPaperApp,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> AnyElement<'a> {
        FileTransferScreen::from(FileTransferScreenProps {
            status: app.file_transfer.status(),
            battery: app.system_status.battery(),
            on_back: cx.listener(InkPaperApp::activate_back),
            on_usb_drive: cx.listener(InkPaperApp::activate_usb_drive),
        })
        .into_any_element()
    }
}
