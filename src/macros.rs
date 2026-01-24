//! Macros to reduce boilerplate in the codebase

/// Macro to generate Display and FromStr implementations for enums
///
/// # Usage
///
/// ```rust,ignore
/// use crate::error::JanusError;
///
/// enum_display_fromstr!(
///     MyEnum,
///     JanusError::InvalidMyEnum,
///     {
///         Variant1 => "variant1",
///         Variant2 => "variant2",
///         Variant3 => "variant_3",
///     }
/// );
/// ```
#[macro_export]
macro_rules! enum_display_fromstr {
    (
        $enum_name:ident,
        $error_variant:path,
        { $($variant:ident => $str:expr),+ $(,)? }
    ) => {
        impl std::fmt::Display for $enum_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $($enum_name::$variant => write!(f, $str),)+
                }
            }
        }

        impl std::str::FromStr for $enum_name {
            type Err = $crate::error::JanusError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s.to_lowercase().as_str() {
                    $($str => Ok($enum_name::$variant),)+
                    _ => Err($error_variant(s.to_string())),
                }
            }
        }
    };
}

/// Macro to generate only Display implementation for enums
///
/// # Usage
///
/// ```rust,ignore
/// enum_display!(
///     MyEnum,
///     {
///         Variant1 => "variant1",
///         Variant2 => "variant2",
///     }
/// );
/// ```
#[macro_export]
macro_rules! enum_display {
    (
        $enum_name:ident,
        { $($variant:ident => $str:expr),+ $(,)? }
    ) => {
        impl std::fmt::Display for $enum_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $($enum_name::$variant => write!(f, $str),)+
                }
            }
        }
    };
}

#[cfg(test)]
mod test {
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum TestEnum {
        A,
        B,
        C,
    }

    enum_display!(TestEnum, { A => "a", B => "b", C => "c" });

    #[test]
    fn test_display() {
        assert_eq!(TestEnum::A.to_string(), "a");
        assert_eq!(TestEnum::B.to_string(), "b");
        assert_eq!(TestEnum::C.to_string(), "c");
    }
}
