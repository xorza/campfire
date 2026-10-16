/// States an enum of variants with no data together with its `ALL`, every variant in the order
/// declared, so neither can lack one. The enum comes first, as written; after it, `const ALL;`,
/// with the visibility and the attributes `ALL` takes; then, if asked for, `const IDENTS;`, the
/// name of each variant as written, in the same order. An enum whose data names its members
/// writes each member's name beside it, `Move => "move"`, and the macro implements `NamedEnum`
/// from them, so a name never leaves its member.
#[macro_export]
macro_rules! enum_all {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident => $data:literal
            ),* $(,)?
        }
        $(#[$all_meta:meta])*
        $all_vis:vis const ALL;
        $(
            $(#[$idents_meta:meta])*
            $idents_vis:vis const IDENTS;
        )?
    ) => {
        $crate::enum_all! {
            $(#[$meta])*
            $vis enum $name {
                $(
                    $(#[$variant_meta])*
                    $variant
                ),*
            }
            $(#[$all_meta])*
            $all_vis const ALL;
            $(
                $(#[$idents_meta])*
                $idents_vis const IDENTS;
            )?
        }

        impl $crate::NamedEnum for $name {
            const TYPE: &'static str = stringify!($name);
            const ALL: &'static [Self] = &Self::ALL;
            const NAMES: &'static [&'static str] = &[$($data),*];
        }
    };
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident $(= $value:expr)?
            ),* $(,)?
        }
        $(#[$all_meta:meta])*
        $all_vis:vis const ALL;
        $(#[$idents_meta:meta])*
        $idents_vis:vis const IDENTS;
    ) => {
        $crate::enum_all! {
            $(#[$meta])*
            $vis enum $name {
                $(
                    $(#[$variant_meta])*
                    $variant $(= $value)?
                ),*
            }
            $(#[$all_meta])*
            $all_vis const ALL;
        }

        impl $name {
            $(#[$idents_meta])*
            $idents_vis const IDENTS: [&'static str; [$(stringify!($variant)),*].len()] =
                [$(stringify!($variant)),*];
        }
    };
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident $(= $value:expr)?
            ),* $(,)?
        }
        $(#[$all_meta:meta])*
        $all_vis:vis const ALL;
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $(
                $(#[$variant_meta])*
                $variant $(= $value)?,
            )*
        }

        impl $name {
            $(#[$all_meta])*
            $all_vis const ALL: [$name; [$(stringify!($variant)),*].len()] =
                [$($name::$variant),*];
        }
    };
}

#[cfg(test)]
mod tests {
    crate::enum_all! {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum Side {
            North,
            /// A variant's attributes stay with it.
            South = 4,
            East,
        }
        const ALL;
    }

    crate::enum_all! {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum Turn {
            Left,
            Right,
        }
        const ALL;
        const IDENTS;
    }

    #[test]
    fn all_holds_every_variant_in_the_order_declared() {
        assert_eq!(Side::ALL, [Side::North, Side::South, Side::East]);
        assert_eq!(Side::ALL.map(|side| side as u8), [0, 4, 5]);
        assert_eq!(Turn::ALL, [Turn::Left, Turn::Right]);
        assert_eq!(Turn::IDENTS, ["Left", "Right"]);
    }
}
