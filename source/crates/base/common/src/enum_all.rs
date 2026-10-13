/// States an enum of variants with no data together with its `ALL`, every variant in the order
/// declared, so neither can lack one. The enum comes first, as written; after it, `const ALL;`,
/// with the visibility and the attributes `ALL` takes.
#[macro_export]
macro_rules! enum_all {
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

    #[test]
    fn all_holds_every_variant_in_the_order_declared() {
        assert_eq!(Side::ALL, [Side::North, Side::South, Side::East]);
        assert_eq!(Side::ALL.map(|side| side as u8), [0, 4, 5]);
    }
}
