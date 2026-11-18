//! Submodule of [`super`]: filter scratch budget unit test.

use super::*;

#[test]
fn filter_scratch_budget_scales_with_primitives() {
    // A 256 MiB layer (64M RGBA pixels) with 16 primitives would need ~9 GiB.
    let layer = 256u64 * 1024 * 1024;
    assert!(filter_scratch_bytes(layer, 1) > MAX_FILTER_SCRATCH_BYTES);
    assert!(filter_scratch_bytes(layer, 16) > filter_scratch_bytes(layer, 1));
    // A small layer with a short chain stays within budget.
    assert!(filter_scratch_bytes(64 * 1024, 2) <= MAX_FILTER_SCRATCH_BYTES);
    // Arithmetic cannot overflow into a false negative.
    assert_eq!(filter_scratch_bytes(u64::MAX, usize::MAX), u64::MAX);
}
