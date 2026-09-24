use inkpaper_ui::prelude::*;

use crate::{
    BatteryStatus, FileTransferStatus,
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

        let (message, detail) = match self.status {
            FileTransferStatus::Selecting => ("", ""),

            FileTransferStatus::Preparing => (
                "Preparing USB Drive...",
                "Eject the drive on your computer to return Home.",
            ),

            FileTransferStatus::Ready => (
                "Connect this reader to your computer",
                "Eject the drive on your computer to return Home.",
            ),

            FileTransferStatus::Error => {
                ("Unable to start USB Drive.", "Press Back to return Home.")
            }
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

                        <text class="text-base text-center wrap max-lines-3">
                            {detail}
                        </text>
                    </div>
                {/if}
            </div>
        }
    }
}
