use inkpaper_ui::prelude::*;

use crate::{
    BatteryStatus,
    components::{
        header::{BackHeader, BackHeaderProps},
        icon::IconKind,
        transfer_mode_row::{TransferModeRow, TransferModeRowProps},
    },
};

#[component]
pub(crate) struct FileTransferScreen {
    battery: Option<BatteryStatus>,
    on_back: Listener<ActivateEvent>,
}

impl RenderOnce for FileTransferScreen {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <BackHeader
                        title="File Transfer"
                        battery={self.battery}
                        on_back={self.on_back}
                    />
                </div>

                <div class="absolute left-0 top-[98px] w-[480px] flex flex-col gap-2.5">
                    <TransferModeRow
                        title="Join a Network"
                        subtitle="Connect to an existing Wi-Fi network"
                        icon={IconKind::Wifi}
                        selected={true}
                    />

                    <TransferModeRow
                        title="Calibre Wireless"
                        subtitle="Transfer books wirelessly with Calibre"
                        icon={IconKind::Library}
                        selected={false}
                    />

                    <TransferModeRow
                        title="Create Hotspot"
                        subtitle="Create a Wi-Fi network for file transfer"
                        icon={IconKind::RadioTower}
                        selected={false}
                    />

                    <TransferModeRow
                        title="USB Drive"
                        subtitle="Access the reader as a USB drive"
                        icon={IconKind::Usb}
                        selected={false}
                    />

                    <div class="h-6 flex items-center pl-5">
                        <text class="font-bold text-xl">
                            {"Nearby Device"}
                        </text>
                    </div>

                    <TransferModeRow
                        title="Receive Nearby Book"
                        subtitle="Receive a book from another nearby reader"
                        icon={IconKind::NearbyTransfer}
                        selected={false}
                    />

                    <TransferModeRow
                        title="Nearby Stats Sync"
                        subtitle="Sync reading stats with another nearby reader"
                        icon={IconKind::ReadingStats}
                        selected={false}
                    />
                </div>
            </div>
        }
    }
}
