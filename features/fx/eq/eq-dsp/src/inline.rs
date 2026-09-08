//! Inline storage for bounded filter-design scratch. The supported runtime
//! orders fit within 32 entries. Expert unbounded math may spill off-thread.
pub type InlineVec<T> = smallvec::SmallVec<[T; 32]>;
macro_rules! inline_vec {
    ($value:expr; $count:expr) => { core::iter::repeat_n($value, $count).collect::<$crate::inline::InlineVec<_>>() };
    ($($value:expr),* $(,)?) => { [$($value),*].into_iter().collect::<$crate::inline::InlineVec<_>>() };
}
pub(crate) use inline_vec;
