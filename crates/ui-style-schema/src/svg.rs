#[macro_export]
macro_rules! inkpaper_svg_schema {
    ($declare:ident) => {
        $declare! {
            @tailwind(Spacing)
            w(width: Pixels);

            @tailwind(Spacing)
            h(height: Pixels);
        }
    };
}
