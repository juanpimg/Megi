//! Batch resource download planning (F7-SAVE-AS / #988).

use super::{DownloadKind, MAX_HOST_CHARS};

/// One planned resource download.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchItem {
    pub url: String,
    pub kind: DownloadKind,
    pub filename: String,
}

/// A bounded plan: at most `max` items; extras are counted as skipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchDownloadPlan {
    pub items: Vec<BatchItem>,
    pub skipped: usize,
}

impl BatchDownloadPlan {
    /// Number of planned downloads.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Classifies a resource URL into a downloadable kind.
pub fn resource_kind(url: &str) -> Option<DownloadKind> {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();
    if path.ends_with(".css") {
        Some(DownloadKind::Css)
    } else if path.ends_with(".js") || path.ends_with(".mjs") {
        Some(DownloadKind::Script)
    } else if path.ends_with(".woff")
        || path.ends_with(".woff2")
        || path.ends_with(".ttf")
        || path.ends_with(".otf")
    {
        Some(DownloadKind::Font)
    } else if path.ends_with(".png")
        || path.ends_with(".jpg")
        || path.ends_with(".jpeg")
        || path.ends_with(".gif")
        || path.ends_with(".webp")
        || path.ends_with(".avif")
    {
        Some(DownloadKind::Image)
    } else {
        None
    }
}

/// Builds a bounded plan from absolute or base-relative resource URLs.
pub fn plan_batch_downloads(urls: &[String], base: Option<&str>, max: usize) -> BatchDownloadPlan {
    let mut items = Vec::new();
    let mut skipped = 0usize;
    for raw in urls {
        let absolute = if raw.starts_with("http://") || raw.starts_with("https://") {
            raw.clone()
        } else if let Some(base) = base {
            format!(
                "{}/{}",
                base.trim_end_matches('/'),
                raw.trim_start_matches('/')
            )
        } else {
            skipped += 1;
            continue;
        };
        let Some(kind) = resource_kind(&absolute) else {
            skipped += 1;
            continue;
        };
        if items.len() >= max {
            skipped += 1;
            continue;
        }
        let leaf = absolute.rsplit('/').next().unwrap_or("resource");
        let mut filename = super::sanitize_save_as_name(leaf, kind.extension());
        if filename.len() > MAX_HOST_CHARS + 8 {
            filename.truncate(MAX_HOST_CHARS + 8);
        }
        items.push(BatchItem {
            url: absolute,
            kind,
            filename,
        });
    }
    BatchDownloadPlan { items, skipped }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_css_js_font_and_image_resources() {
        assert_eq!(
            resource_kind("https://x.test/a.CSS?v=1"),
            Some(DownloadKind::Css)
        );
        assert_eq!(
            resource_kind("https://x.test/app.min.js"),
            Some(DownloadKind::Script)
        );
        assert_eq!(
            resource_kind("https://x.test/font.woff2"),
            Some(DownloadKind::Font)
        );
        assert_eq!(
            resource_kind("https://x.test/pic.webp"),
            Some(DownloadKind::Image)
        );
        assert_eq!(resource_kind("https://x.test/data.json"), None);
    }

    #[test]
    fn plan_resolves_relative_urls_and_respects_the_cap() {
        let urls: Vec<String> = [
            "/a.css",
            "b.JS",
            "https://cdn.test/f.woff2",
            "/x.bin",
            "/d.css",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        let plan = plan_batch_downloads(&urls, Some("https://site.test/dir/"), 3);
        assert_eq!(plan.len(), 3);
        assert_eq!(plan.skipped, 2, "unsupported type and over-cap item");
        assert_eq!(plan.items[0].url, "https://site.test/dir/a.css");
        assert_eq!(plan.items[0].kind, DownloadKind::Css);
        assert_eq!(plan.items[0].filename, "a.css");
        assert_eq!(plan.items[1].url, "https://site.test/dir/b.JS");
        assert_eq!(plan.items[2].kind, DownloadKind::Font);
    }
}
