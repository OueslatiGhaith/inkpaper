pub const MAX_FIELDS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    name: &'static str,
}

impl Field {
    #[doc(hidden)]
    pub const fn new(name: &'static str) -> Self {
        assert_single_line(name);

        Self { name }
    }

    pub const fn name(self) -> &'static str {
        self.name
    }
}

#[derive(Debug)]
pub struct Metadata {
    name: &'static str,
    target: &'static str,
    fields: &'static [Field],
}

impl Metadata {
    const fn new(name: &'static str, target: &'static str, fields: &'static [Field]) -> Self {
        Self {
            name,
            target,
            fields,
        }
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }

    pub const fn target(&self) -> &'static str {
        self.target
    }

    pub const fn fields(&self) -> &'static [Field] {
        self.fields
    }
}

#[derive(Debug)]
pub struct Callsite {
    metadata: Metadata,
}

impl Callsite {
    #[doc(hidden)]
    pub const fn new(name: &'static str, target: &'static str, fields: &'static [Field]) -> Self {
        assert!(
            fields.len() <= MAX_FIELDS,
            "trace callsites support at most two fields",
        );

        assert_single_line(name);
        assert_single_line(target);

        Self {
            metadata: Metadata::new(name, target, fields),
        }
    }

    pub const fn metadata(&self) -> &Metadata {
        &self.metadata
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ValueKind {
    Unsigned = 0,
    Signed = 1,
    Bool = 2,
}

impl ValueKind {
    pub(crate) const fn from_bits(bits: u8) -> Option<Self> {
        match bits {
            0 => Some(Self::Unsigned),
            1 => Some(Self::Signed),
            2 => Some(Self::Bool),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Value {
    raw: u32,
    kind: ValueKind,
}

impl Value {
    #[doc(hidden)]
    pub const EMPTY: Self = Self::unsigned(0);

    pub const fn unsigned(value: u32) -> Self {
        Self {
            raw: value,
            kind: ValueKind::Unsigned,
        }
    }

    pub const fn signed(value: i32) -> Self {
        Self {
            raw: value as u32,
            kind: ValueKind::Signed,
        }
    }

    pub const fn boolean(value: bool) -> Self {
        Self {
            raw: value as u32,
            kind: ValueKind::Bool,
        }
    }

    pub const fn kind(self) -> ValueKind {
        self.kind
    }

    pub const fn as_u32(self) -> Option<u32> {
        match self.kind {
            ValueKind::Unsigned => Some(self.raw),
            _ => None,
        }
    }

    pub const fn as_i32(self) -> Option<i32> {
        match self.kind {
            ValueKind::Signed => Some(self.raw as i32),
            _ => None,
        }
    }

    pub const fn as_bool(self) -> Option<bool> {
        match self.kind {
            ValueKind::Bool => Some(self.raw != 0),
            _ => None,
        }
    }

    #[doc(hidden)]
    pub const fn raw(self) -> u32 {
        self.raw
    }

    pub(crate) const fn from_raw(kind: ValueKind, raw: u32) -> Self {
        Self { raw, kind }
    }
}

pub trait IntoValue {
    fn into_value(self) -> Value;
}

const fn assert_single_line(value: &str) {
    let bytes = value.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        assert!(
            bytes[index] != b'\n' && bytes[index] != b'\r',
            "trace metadata must not contain line breaks",
        );

        index += 1;
    }
}

macro_rules! impl_unsigned_value {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl IntoValue for $ty {
                #[inline(always)]
                fn into_value(self) -> Value {
                    Value::unsigned(u32::from(self))
                }
            }
        )+
    };
}

impl_unsigned_value!(u8, u16, u32);

impl IntoValue for usize {
    #[inline(always)]
    fn into_value(self) -> Value {
        Value::unsigned(u32::try_from(self).unwrap_or(u32::MAX))
    }
}

macro_rules! impl_signed_value {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl IntoValue for $ty {
                #[inline(always)]
                fn into_value(self) -> Value {
                    Value::signed(i32::from(self))
                }
            }
        )+
    };
}

impl_signed_value!(i8, i16, i32);

impl IntoValue for isize {
    #[inline(always)]
    fn into_value(self) -> Value {
        let value = i32::try_from(self).unwrap_or(if self.is_negative() {
            i32::MIN
        } else {
            i32::MAX
        });

        Value::signed(value)
    }
}

impl IntoValue for bool {
    #[inline(always)]
    fn into_value(self) -> Value {
        Value::boolean(self)
    }
}

#[doc(hidden)]
#[inline(always)]
pub fn into_value<T>(value: T) -> Value
where
    T: IntoValue,
{
    value.into_value()
}
