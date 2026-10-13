//! Macros for generating behavior enums and their implementations

/// Generate a behavior enum with automatic implementations of `BehaviorId` and `BehaviorDisplay`.
///
/// # Syntax
///
/// ```rust,ignore
/// define_behavior_enum! {
///     pub enum MyBehavior {
///         NoAction => (0, "No action"),
///         SomeAction => (1, "Some action"),
///         AnotherAction => (2, "Another action"),
///     }
/// }
/// ```
///
/// The macro will:
/// - Generate the enum with `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]`
/// - Automatically add `Unknown(u32)` variant to handle unknown IDs
/// - Implement `BehaviorId` with `from_behavior_id` and `to_behavior_id`
/// - Implement `BehaviorDisplay` with `display_name`
///
/// # Notes
/// - The first variant should typically be `NoAction` with ID 0
/// - The `Unknown(u32)` variant is automatically added by the macro
/// - IDs can be non-sequential (e.g., `(62, "Some action")`)
#[macro_export]
macro_rules! define_behavior_enum {
    (
        $(#[$enum_meta:meta])*
        $vis:vis enum $enum_name:ident {
            $(
                $variant:ident => $(($id:expr, $display:expr))?
            ),* $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $vis enum $enum_name {
            $(
                $variant,
            )*
            Unknown(u32),
        }

        impl $crate::input::mouse_modifiers::behaviors::shared::traits::BehaviorId for $enum_name {
            fn from_behavior_id(behavior_id: u32) -> Self {
                match behavior_id {
                    $(
                        $(
                            $id => $enum_name::$variant,
                        )?
                    )*
                    id => $enum_name::Unknown(id),
                }
            }

            fn to_behavior_id(&self) -> u32 {
                match self {
                    $(
                        $enum_name::$variant => {
                            $(
                                $id
                            )?
                        },
                    )*
                    $enum_name::Unknown(id) => *id,
                }
            }
        }

        impl $crate::input::mouse_modifiers::behaviors::shared::traits::BehaviorDisplay for $enum_name {
            fn display_name(&self) -> &'static str {
                match self {
                    $(
                        $enum_name::$variant => {
                            $(
                                $display
                            )?
                        },
                    )*
                    $enum_name::Unknown(_) => "Unknown behavior",
                }
            }
        }
    };
}
