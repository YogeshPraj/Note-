//! Checked numeric conversions.
//!
//! Scintilla and the file formats mix `usize`, `isize` and `i32`. These helpers replace `as` casts: a value that does not
//! fit saturates (a negative number becomes 0 for unsigned targets, too large a number becomes the maximum) instead of
//! wrapping into a wrong position.

macro_rules! saturating {
    ($($name:ident => $t:ty),* $(,)?) => {$(
        #[doc = concat!("Convert to `", stringify!($t), "`, saturating at its bounds.")]
        pub fn $name<T: TryInto<i128>>(x: T) -> $t {
            match x.try_into() {
                Ok(v) => <$t>::try_from(v).unwrap_or(if v < 0 { <$t>::MIN } else { <$t>::MAX }),
                Err(_) => <$t>::MAX,
            }
        }
    )*};
}

saturating! {
    to_usize => usize,
    to_isize => isize,
    to_u8 => u8,
    to_u16 => u16,
    to_u32 => u32,
    to_u64 => u64,
    to_i32 => i32,
    to_i64 => i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_becomes_zero_for_unsigned() {
        assert_eq!(to_usize(-1_isize), 0);
        assert_eq!(to_u32(-5_i32), 0);
    }

    #[test]
    fn too_large_saturates() {
        assert_eq!(to_isize(usize::MAX), isize::MAX);
        assert_eq!(to_u8(300_u32), u8::MAX);
        assert_eq!(to_i32(u64::MAX), i32::MAX);
    }

    #[test]
    fn in_range_is_exact() {
        assert_eq!(to_usize(42_isize), 42);
        assert_eq!(to_isize(42_usize), 42);
        assert_eq!(to_i32(-7_i64), -7);
        assert_eq!(to_u8(255_u32), 255);
    }
}
