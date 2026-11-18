#[allow(unused_imports)]
use super::*;

#[cfg(test)]
mod palette_tests {
    use super::*;

    #[test]
    pub(crate) fn out_of_range_palette_indices_are_rejected() {
        let palette = vec![0xFF00_00FFu32];
        let valid = Transform::Indexing {
            width: 1,
            height: 1,
            bits: 0,
            palette: palette.clone(),
        };
        assert_eq!(valid.apply(vec![0]).expect("index 0"), vec![0xFF00_00FF]);
        let invalid = Transform::Indexing {
            width: 1,
            height: 1,
            bits: 0,
            palette,
        };
        match invalid.apply(vec![1 << 8]) {
            Err(ImageError::InvalidData { reason }) => {
                assert!(reason.contains("palette index out of range"), "{reason}");
            }
            other => panic!("expected InvalidData, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod animated_frame_budget_tests {
    use super::*;

    #[test]
    pub(crate) fn aggregate_decoded_frame_budget_is_enforced() {
        // A 512x512 RGBA canvas is 1 MiB; 256 frames exactly fill the 256 MiB budget.
        let canvas = 512 * 512 * 4;
        assert!(check_animated_frame_budget(0, canvas).is_ok());
        assert!(check_animated_frame_budget(255, canvas).is_ok());
        assert!(matches!(
            check_animated_frame_budget(256, canvas),
            Err(ImageError::LimitExceeded { .. })
        ));
    }

    #[test]
    pub(crate) fn aggregate_frame_budget_does_not_overflow_on_saturating_sizes() {
        assert!(matches!(
            check_animated_frame_budget(usize::MAX, usize::MAX),
            Err(ImageError::LimitExceeded { .. })
        ));
    }
}
