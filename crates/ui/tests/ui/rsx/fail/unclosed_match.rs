use inkpaper_ui::prelude::*;

fn main() {
    let row = 1;

    let _ = rsx! {
        <div>
            {#match row}
                {:case 0}
                    <text>"None"</text>
                {:case _}
                    <text>"Some"</text>
        </div>
    };
}
