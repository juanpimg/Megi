use super::*;

// Gap-fill DOM/BOM bindings (split verbatim from the former monolithic gap_apis.rs).
mod canvas;
mod frames;
mod install;
mod nodes;
mod slots;

pub(crate) use canvas::*;
pub(crate) use frames::*;
pub(crate) use install::*;
pub(crate) use nodes::*;
pub(crate) use slots::*;

#[cfg(test)]
mod dataset_name_tests {
    use super::*;

    #[test]
    pub(crate) fn dataset_camel_case_follows_domstringmap_rules() {
        assert_eq!(camel_case_attribute("data-foo-bar"), Some("fooBar".into()));
        assert_eq!(camel_case_attribute("data-a-é"), Some("a-é".into()));
        assert_eq!(camel_case_attribute("data-x-1"), Some("x-1".into()));
        // Per HTML spec, a dash followed by an ASCII lower alpha is removed even
        // when it is the first character of the suffix: "data--foo" -> "Foo".
        assert_eq!(camel_case_attribute("data--foo"), Some("Foo".into()));
        assert_eq!(camel_case_attribute("data-ß"), Some("ß".into()));
        assert_eq!(camel_case_attribute("role"), None);
    }
}
