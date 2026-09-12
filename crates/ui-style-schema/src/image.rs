#[macro_export]
macro_rules! inkpaper_image_schema {
    ($declare:ident) => {
        $declare! {
            @tailwind(Spacing)
            w(width: Pixels);

            @tailwind(Spacing)
            h(height: Pixels);

            @tailwind(class = "object-contain")
            contain;

            @tailwind(class = "object-cover")
            cover;

            @tailwind(class = "object-fill")
            fill;

            @tailwind(class = "object-none")
            native;

            @tailwind(class = "grayscale")
            grayscale;

            @tailwind(class = "invert")
            invert;
        }
    };
}
