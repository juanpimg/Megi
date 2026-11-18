//! Fetching and decoding one external stylesheet body.
//!
//! Moved verbatim from `lib.rs`.

use megi_net::{ResourceLoader, Scheme, Url};

use crate::charset;
use crate::{RenderError, RenderOptions};

/// Loads one stylesheet body honouring `--no-network`, optional SRI, and the per-sheet size cap.
///
/// Missing/unreachable sheets (or SRI mismatches) are non-fatal (browser behaviour) and reported as warnings.
pub(crate) fn load_stylesheet_text(
    url: &Url,
    integrity: Option<&str>,
    loader: &mut ResourceLoader,
    options: &RenderOptions,
    warnings: &mut Vec<String>,
    use_cors: bool,
) -> Result<Option<String>, RenderError> {
    if options.no_network && matches!(url.scheme(), Scheme::Http | Scheme::Https) {
        warnings.push(format!(
            "skipping external stylesheet {url} (--no-network is set)"
        ));
        return Ok(None);
    }
    match loader.load_with_mode_checked(url, true, use_cors) {
        Ok(resource) => {
            if resource.bytes.len() > options.max_css_bytes_per_sheet {
                // #654: skip the sheet with a warning instead of failing the render.
                warnings.push(format!(
                    "skipping stylesheet {url}: {} bytes exceeds the {} byte cap",
                    resource.bytes.len(),
                    options.max_css_bytes_per_sheet
                ));
                return Ok(None);
            }
            if let Some(integrity) = integrity
                && let Err(error) =
                    megi_net::integrity::verify_integrity(&resource.bytes, integrity)
            {
                warnings.push(format!(
                    "skipping stylesheet {url}: integrity check failed: {error}"
                ));
                return Ok(None);
            }
            {
                // CSS Syntax §3.2: strip the UTF-8 BOM and honour `@charset`/the resource
                // charset before tokenizing (#1449). An unsupported encoding is non-fatal:
                // warn and fall back to a BOM-stripped lossy UTF-8 decode, as before.
                let decoded =
                    match charset::decode_stylesheet(&resource.bytes, &resource.content_type) {
                        Ok(decoded) => decoded,
                        Err(error) => {
                            warnings.push(format!("stylesheet {url}: {error}; decoding as UTF-8"));
                            let bytes = resource
                                .bytes
                                .strip_prefix(b"\xEF\xBB\xBF")
                                .unwrap_or(&resource.bytes);
                            charset::DecodedStylesheet {
                                text: String::from_utf8_lossy(bytes).into_owned(),
                                charset: "utf-8".to_owned(),
                            }
                        }
                    };
                if decoded.text.contains('\u{FFFD}') {
                    warnings.push(format!("stylesheet {url}: non-UTF-8 bytes replaced"));
                }
                Ok(Some(decoded.text))
            }
        }
        Err(error) => {
            warnings.push(format!("skipping stylesheet {url}: {error}"));
            Ok(None)
        }
    }
}
