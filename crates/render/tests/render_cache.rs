//! Integration tests for `RenderCache` (layout fingerprint cache & CSS parse cache, #947, #990).

use render::{RenderCache, RenderOptions, Viewport, render_url, render_url_cached};

#[test]
fn render_url_cached_reuses_layout_and_produces_byte_identical_png() {
    let url = "data:text/html,<!doctype html><style>h1{color:%23224488}</style><h1>Cached</h1><p>Hello Megi</p>";
    let opts = RenderOptions {
        viewport: Viewport::new(400, 300).unwrap(),
        no_network: true,
        ..RenderOptions::default()
    };

    let cold = render_url(url, &opts).unwrap();
    assert!(!cold.stats.cache_layout_hit);

    let mut cache = RenderCache::new();
    let first = render_url_cached(url, &opts, &mut cache).unwrap();
    assert!(!first.stats.cache_layout_hit);
    assert_eq!(cache.stats().layout_misses, 1);
    assert_eq!(cache.stats().layout_hits, 0);

    let second = render_url_cached(url, &opts, &mut cache).unwrap();
    assert!(second.stats.cache_layout_hit);
    assert_eq!(cache.stats().layout_misses, 1);
    assert_eq!(cache.stats().layout_hits, 1);
    assert_eq!(first.png, second.png);
    assert_eq!(cold.png, second.png);

    // Changing viewport misses the cache.
    let opts2 = RenderOptions {
        viewport: Viewport::new(500, 300).unwrap(),
        ..opts
    };
    let third = render_url_cached(url, &opts2, &mut cache).unwrap();
    assert!(!third.stats.cache_layout_hit);
    assert_eq!(cache.stats().layout_misses, 2);
}

#[test]
fn parse_stylesheet_cache_hits_on_identical_css() {
    let mut cache = RenderCache::new();
    let s1 = cache.parse_stylesheet("body { color: red; }");
    let s2 = cache.parse_stylesheet("body { color: red; }");
    assert_eq!(s1.rules.len(), s2.rules.len());
    assert_eq!(cache.stats().css_misses, 1);
    assert_eq!(cache.stats().css_hits, 1);
}
