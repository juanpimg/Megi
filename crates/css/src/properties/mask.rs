//! Computed `mask-image` (CSS Masking 1 §6.1).
//!
//! Only a single image layer is supported so far: the first `url()` in
//! `mask-image` (or its `-webkit-` alias). `none`, gradients and unsupported
//! values compute to `None`, i.e. no mask. URL parsing is shared with
//! `background-image` so data URIs behave identically in both properties.

use crate::rule::Declaration;

use super::parse::parse_image_value;

/// Returns the raw (unresolved) mask image URL of this style, if any.
///
/// CSS Masking 1 §6.1: the standard property wins over the prefixed alias when
/// both are present. Both accept `none` and a comma-separated image list; this
/// minimal implementation retains only the first usable `url()`.
pub(crate) fn compute_mask_image(entries: &[(&str, &Declaration)]) -> Option<String> {
    let standard = entries
        .iter()
        .find_map(|(name, declaration)| (*name == "mask-image").then_some(*declaration));
    let prefixed = entries
        .iter()
        .find_map(|(name, declaration)| (*name == "-webkit-mask-image").then_some(*declaration));
    standard
        .or(prefixed)
        .and_then(|declaration| parse_image_value(&declaration.raw_value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl(value: &str) -> Declaration {
        Declaration {
            name: "mask-image".to_owned(),
            value: Vec::new(),
            important: false,
            raw_value: value.to_owned(),
        }
    }

    #[test]
    fn mask_image_accepts_a_data_uri() {
        let uri = "data:image/png;base64,iVBORw0KGgo=";
        let declaration = decl(&format!("url(\"{uri}\")"));
        assert_eq!(
            compute_mask_image(&[("mask-image", &declaration)]),
            Some(uri.to_owned())
        );
    }

    #[test]
    fn webkit_alias_is_used_when_standard_is_absent() {
        let declaration = decl("url(mask.svg)");
        assert_eq!(
            compute_mask_image(&[("-webkit-mask-image", &declaration)]),
            Some("mask.svg".to_owned())
        );
    }

    #[test]
    fn standard_property_wins_over_the_alias() {
        let standard = decl("url(standard.svg)");
        let prefixed = decl("url(prefixed.svg)");
        assert_eq!(
            compute_mask_image(&[("-webkit-mask-image", &prefixed), ("mask-image", &standard),]),
            Some("standard.svg".to_owned())
        );
    }

    #[test]
    fn none_and_gradients_compute_to_no_mask() {
        let none = decl("none");
        let gradient = decl("linear-gradient(red, blue)");
        assert_eq!(compute_mask_image(&[("mask-image", &none)]), None);
        assert_eq!(compute_mask_image(&[("mask-image", &gradient)]), None);
    }
}
