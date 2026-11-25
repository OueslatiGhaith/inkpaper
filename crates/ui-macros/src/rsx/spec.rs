use std::sync::OnceLock;

use heck::ToKebabCase;
use inkpaper_ui_style_schema::tailwind::ValueKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UtilityReceiver {
    Styled,
    TextStyled,
    Image,
    Svg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgumentKind {
    None,
    Pixels,
    U16,
    FontWeight,
    Unsupported,
}

#[derive(Debug, Clone)]
pub struct UtilitySpec {
    pub rust_name: &'static str,
    pub receiver: UtilityReceiver,
    pub argument_types: &'static [&'static str],
    pub classname: String,
    pub argument_kind: ArgumentKind,
    pub value_kind: Option<ValueKind>,
}

impl UtilitySpec {
    fn new(
        rust_name: &'static str,
        receiver: UtilityReceiver,
        argument_types: &'static [&'static str],
        classname_override: Option<&'static str>,
        value_kind: Option<ValueKind>,
    ) -> Self {
        let classname = classname_override
            .map(str::to_owned)
            .unwrap_or_else(|| rust_name.to_kebab_case());
        let argument_kind = match argument_types {
            [] => ArgumentKind::None,
            [argument] => classify_argument_type(argument),
            _ => ArgumentKind::Unsupported,
        };

        Self {
            rust_name,
            receiver,
            argument_types,
            classname,
            argument_kind,
            value_kind,
        }
    }

    pub fn classname(&self) -> &str {
        &self.classname
    }

    pub fn argument_kind(&self) -> ArgumentKind {
        self.argument_kind
    }
}

fn classify_argument_type(argument: &str) -> ArgumentKind {
    let normalized = argument
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();

    match normalized.as_str() {
        "Pixels" | "implInto<Pixels>" | "implInto<Length>" | "implInto<LineHeight>" => {
            ArgumentKind::Pixels
        }
        "u16" => ArgumentKind::U16,
        "FontWeight" => ArgumentKind::FontWeight,
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

macro_rules! collect_style_specs {
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

            $specs.push(UtilitySpec::new(
                stringify!($method_name),
                $receiver,
                &[ $( stringify!($argument_type) ),+ ],
                classname_override,
                value_kind,
            ));
        }

        collect_style_specs!($specs, $receiver; $( $rest )*);
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

            $specs.push(UtilitySpec::new(
                stringify!($method_name),
                $receiver,
                &[],
                classname_override,
                value_kind,
            ));
        }

        collect_style_specs!($specs, $receiver; $( $rest )*);
    };

    (
        $specs:ident,
        $receiver:expr;
        $method_name:ident( $( $argument_name:ident: $argument_type:ty ),+ $(,)? ) => $implementation:expr;
        $($rest:tt)*
    ) => {
        $specs.push(UtilitySpec::new(
            stringify!($method_name),
            $receiver,
            &[ $( stringify!($argument_type) ),+ ],
            None,
            None,
        ));

        collect_style_specs!(
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
        $specs.push(UtilitySpec::new(
            stringify!($method_name),
            $receiver,
            &[],
            None,
            None,
        ));

        collect_style_specs!(
            $specs,
            $receiver;
            $($rest)*
        );
    };
}

macro_rules! declare_style_specs {
    (
        $(#[$attr:meta])*
        pub struct $style_name:ident {
            $(
                $invalidation:ident {
                    $(
                        $field_name:ident:
                        $field_ty:ty =
                        $field_default:expr => {
                            $($field_utilities:tt)*
                        }
                    ),*
                    $(,)?
                }
            )*

            @text {
                $(
                    $text_invalidation:ident {
                        $(
                            $text_name:ident:
                            $text_ty:ty =
                            $text_default:expr => {
                                $($text_utilities:tt)*
                            }
                        ),*
                        $(,)?
                    }
                )*
            }
        }
    ) => {
        fn push_style_specs(
            specs: &mut Vec<UtilitySpec>,
        ) {
            $(
                $(
                    collect_style_specs!(
                        specs,
                        UtilityReceiver::Styled;
                        $($field_utilities)*
                    );
                )*
            )*

            $(
                $(
                    collect_style_specs!(
                        specs,
                        UtilityReceiver::TextStyled;
                        $($text_utilities)*
                    );
                )*
            )*
        }
    };
}

macro_rules! collect_image_specs {
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

            $specs.push(UtilitySpec::new(
                stringify!($method_name),
                UtilityReceiver::Image,
                &[$(stringify!($argument_type)),+],
                classname_override,
                value_kind,
            ));
        }

        collect_image_specs!(
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
            let (value_kind, classname_override) = tailwind_metadata!($($tailwind)*);

            $specs.push(UtilitySpec::new(
                stringify!($method_name),
                UtilityReceiver::Image,
                &[],
                classname_override,
                value_kind,
            ));
        }

        collect_image_specs!($specs; $($rest)*);
    };

    (
        $specs:ident;
        $method_name:ident($($argument_name:ident: $argument_type:ty),+ $(,)? );
        $($rest:tt)*
    ) => {
        $specs.push(UtilitySpec::new(
            stringify!($method_name),
            UtilityReceiver::Image,
            &[$(stringify!($argument_type)),+],
            None,
            None,
        ));

        collect_image_specs!($specs; $($rest)*);
    };

    (
        $specs:ident;
        $method_name:ident;
        $($rest:tt)*
    ) => {
        $specs.push(UtilitySpec::new(
            stringify!($method_name),
            UtilityReceiver::Image,
            &[],
            None,
            None,
        ));

        collect_image_specs!($specs; $($rest)*);
    };
}

macro_rules! declare_image_specs {
    ( $( $utilities:tt )* ) => {
        fn push_image_specs(specs: &mut Vec<UtilitySpec>) {
            collect_image_specs!(specs; $($utilities)*);
        }
    };
}

macro_rules! collect_svg_specs {
    ($specs:ident;) => {};

    (
        $specs:ident;

        @tailwind($($tailwind:tt)*)
        $method_name:ident($($argument_name:ident: $argument_type:ty),+ $(,)?);
        $($rest:tt)*
    ) => {
        {
            let (value_kind, classname_override) = tailwind_metadata!($($tailwind)*);

            $specs.push(UtilitySpec::new(
                stringify!($method_name),
                UtilityReceiver::Svg,
                &[$(stringify!($argument_type)),+],
                classname_override,
                value_kind,
            ));
        }

        collect_svg_specs!($specs; $($rest)*);
    };
}

macro_rules! declare_svg_specs {
    ( $( $utilities:tt )* ) => {
        fn push_svg_specs(specs: &mut Vec<UtilitySpec>) {
            collect_svg_specs!(specs; $($utilities)*);
        }
    };
}

inkpaper_ui_style_schema::inkpaper_style_schema!(declare_style_specs);
inkpaper_ui_style_schema::inkpaper_image_schema!(declare_image_specs);
inkpaper_ui_style_schema::inkpaper_svg_schema!(declare_svg_specs);

#[allow(clippy::vec_init_then_push)]
pub fn utility_specs() -> &'static [UtilitySpec] {
    static SPECS: OnceLock<Vec<UtilitySpec>> = OnceLock::new();

    SPECS.get_or_init(|| {
        let mut specs = Vec::new();

        push_style_specs(&mut specs);
        push_image_specs(&mut specs);
        push_svg_specs(&mut specs);

        specs
    })
}
