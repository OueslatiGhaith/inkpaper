use heck::ToKebabCase;
use inkpaper_ui_style_schema::tailwind::ValueKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UtilityReceiver {
    Styled,
    TextStyled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgumentKind {
    None,
    Pixels,
    U16,
    Unsupported,
}

#[derive(Debug, Clone)]
pub struct UtilitySpec {
    pub rust_name: &'static str,
    pub receiver: UtilityReceiver,
    pub argument_types: Vec<&'static str>,
    pub classname_override: Option<&'static str>,
    pub value_kind: Option<ValueKind>,
}

impl UtilitySpec {
    pub fn classname(&self) -> String {
        self.classname_override
            .map(str::to_owned)
            .unwrap_or_else(|| self.rust_name.to_kebab_case())
    }

    pub fn argument_kind(&self) -> ArgumentKind {
        match self.argument_types.as_slice() {
            [] => ArgumentKind::None,
            [argument] => classify_argument_type(argument),
            _ => ArgumentKind::Unsupported,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageUtilitySpec {
    pub rust_name: &'static str,
    pub argument_types: Vec<&'static str>,
    pub classname_override: Option<&'static str>,
    pub value_kind: Option<ValueKind>,
}

impl ImageUtilitySpec {
    pub fn classname(&self) -> String {
        self.classname_override
            .map(str::to_owned)
            .unwrap_or_else(|| self.rust_name.to_kebab_case())
    }

    pub fn argument_kind(&self) -> ArgumentKind {
        match self.argument_types.as_slice() {
            [] => ArgumentKind::None,
            [argument] => classify_argument_type(argument),
            _ => ArgumentKind::Unsupported,
        }
    }
}

fn classify_argument_type(argument: &str) -> ArgumentKind {
    let normalized = argument
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();

    match normalized.as_str() {
        "Pixels" | "implInto<Pixels>" | "implInto<Length>" | "implInto<LineHeight>" => {
            ArgumentKind::Pixels
        }
        "u16" => ArgumentKind::U16,
        _ => ArgumentKind::Unsupported,
    }
}

macro_rules! tailwind_metadata {
    () => {
        (None::<ValueKind>, None::<&'static str>)
    };

    ($kind:ident) => {
        (Some(ValueKind::$kind), None::<&'static str>)
    };

    ($kind:ident, class = $classname:literal) => {
        (Some(ValueKind::$kind), Some($classname))
    };

    (class = $classname:literal) => {
        (None::<ValueKind>, Some($classname))
    };
}

macro_rules! collect_utility_specs {
    ($specs:ident, $receiver:expr;) => {};

    (
        $specs:ident,
        $receiver:expr;
        @tailwind($($tailwind:tt)*)
        $method_name:ident( $( $argument_name:ident: $argument_type:ty ),+ $(,)? ) => $implementation:expr;
        $($rest:tt)*
    ) => {
        {
            let (value_kind, classname_override) = tailwind_metadata!($($tailwind)*);

            $specs.push(UtilitySpec {
                rust_name: stringify!($method_name),
                receiver: $receiver,
                argument_types: vec![ $( stringify!($argument_type) ),+ ],
                classname_override,
                value_kind,
            });
        }

        collect_utility_specs!($specs, $receiver; $( $rest )*);
    };

    (
        $specs:ident,
        $receiver:expr;
        @tailwind($($tailwind:tt)*)
        $method_name:ident => $implementation:expr;
        $($rest:tt)*
    ) => {
        {
            let (value_kind, classname_override) = tailwind_metadata!($($tailwind)*);

            $specs.push(UtilitySpec {
                rust_name: stringify!($method_name),
                receiver: $receiver,
                argument_types: Vec::new(),
                classname_override,
                value_kind,
            });
        }

        collect_utility_specs!($specs, $receiver; $( $rest )*);
    };

    (
        $specs:ident,
        $receiver:expr;
        $method_name:ident( $( $argument_name:ident: $argument_type:ty ),+ $(,)? ) => $implementation:expr;
        $($rest:tt)*
    ) => {
        $specs.push(UtilitySpec {
            rust_name: stringify!($method_name),
            receiver: $receiver,
            argument_types: vec![ $( stringify!($argument_type) ),+ ],
            classname_override: None,
            value_kind: None,
        });

        collect_utility_specs!(
            $specs,
            $receiver;
            $($rest)*
        );
    };

    (
        $specs:ident,
        $receiver:expr;
        $method_name:ident => $implementation:expr;
        $($rest:tt)*
    ) => {
        $specs.push(UtilitySpec {
            rust_name: stringify!($method_name),
            receiver: $receiver,
            argument_types: Vec::new(),
            classname_override: None,
            value_kind: None,
        });

        collect_utility_specs!(
            $specs,
            $receiver;
            $($rest)*
        );
    };
}

macro_rules! collect_image_utility_specs {
    ($specs:ident;) => {};

    (
        $specs:ident;

        @tailwind($($tailwind:tt)*)
        $method_name:ident(
            $( $argument_name:ident: $argument_type:ty ),+ $(,)?
        );

        $($rest:tt)*
    ) => {
        {
            let (value_kind, classname_override) =
                tailwind_metadata!($($tailwind)*);

            $specs.push(ImageUtilitySpec {
                rust_name: stringify!($method_name),
                argument_types: vec![
                    $(stringify!($argument_type)),+
                ],
                classname_override,
                value_kind,
            });
        }

        collect_image_utility_specs!(
            $specs;
            $($rest)*
        );
    };

    (
        $specs:ident;

        @tailwind($($tailwind:tt)*)
        $method_name:ident;

        $($rest:tt)*
    ) => {
        {
            let (value_kind, classname_override) =
                tailwind_metadata!($($tailwind)*);

            $specs.push(ImageUtilitySpec {
                rust_name: stringify!($method_name),
                argument_types: Vec::new(),
                classname_override,
                value_kind,
            });
        }

        collect_image_utility_specs!(
            $specs;
            $($rest)*
        );
    };
}

macro_rules! declare_image_utility_specs {
    (
        $($utilities:tt)*
    ) => {
        #[allow(clippy::vec_init_then_push)]
        pub fn image_utility_specs() -> Vec<ImageUtilitySpec> {
            let mut specs = Vec::new();

            collect_image_utility_specs!(
                specs;
                $($utilities)*
            );

            specs
        }
    };
}

macro_rules! declare_utility_specs {
    (
        $(#[$attr:meta])*
        pub struct $style_name:ident {
            $(
                $invalidation:ident {
                    $(
                        $field_name:ident: $field_ty:ty = $field_default:expr => {
                            $($field_utilities:tt)*
                        }
                    ),* $(,)?
                }
            )*

            @text {
                $(
                    $text_invalidation:ident {
                        $(
                            $text_name:ident: $text_ty:ty = $text_default:expr => {
                                $($text_utilities:tt)*
                            }
                        ),* $(,)?
                    }
                )*
            }
        }
    ) => {
        #[allow(clippy::vec_init_then_push)]
        pub fn utility_specs() -> Vec<UtilitySpec> {
            let mut specs = Vec::new();

            $( $( collect_utility_specs!( specs, UtilityReceiver::Styled; $( $field_utilities )* ); )* )*

            $( $( collect_utility_specs!(specs, UtilityReceiver::TextStyled; $( $text_utilities )* ); )* )*

            specs
        }
    };
}

inkpaper_ui_style_schema::inkpaper_style_schema!(declare_utility_specs);
inkpaper_ui_style_schema::inkpaper_image_schema!(declare_image_utility_specs);
