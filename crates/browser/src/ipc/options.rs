//! Serializable subset of `RenderOptions` and cookie state for `--megi-ipc-worker` (#2165).

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use megi_webview::net::{Cookie, CookieJar, SameSite, Url};
use megi_webview::render::RenderOptions;

/// Wire mirror of [`SameSite`] without pulling `serde` derives into `megi-net`.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IpcSameSite {
    Strict,
    #[default]
    Lax,
    None,
}

impl From<SameSite> for IpcSameSite {
    fn from(value: SameSite) -> Self {
        match value {
            SameSite::Strict => Self::Strict,
            SameSite::Lax => Self::Lax,
            SameSite::None => Self::None,
        }
    }
}

impl IpcSameSite {
    const fn as_header_str(self) -> &'static str {
        match self {
            Self::Strict => "Strict",
            Self::Lax => "Lax",
            Self::None => "None",
        }
    }
}

/// Serializable RFC 6265 cookie entry synchronized between parent and child jars.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IpcCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub host_only: bool,
    pub path: String,
    pub secure: bool,
    pub same_site: IpcSameSite,
    pub http_only: bool,
    pub expires_epoch_secs: Option<u64>,
}

impl IpcCookie {
    /// Captures a stored cookie for transport across the IPC boundary.
    pub fn from_cookie(cookie: &Cookie) -> Self {
        let expires_epoch_secs = cookie.expires.map(|instant| {
            instant
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
        });
        Self {
            name: cookie.name.clone(),
            value: cookie.value.clone(),
            domain: cookie.domain.clone(),
            host_only: cookie.host_only,
            path: cookie.path.clone(),
            secure: cookie.secure,
            same_site: IpcSameSite::from(cookie.same_site),
            http_only: cookie.http_only,
            expires_epoch_secs,
        }
    }

    /// Replays serialized cookies into `jar` via `CookieJar::store_set_cookie`.
    ///
    /// `HttpOnly` is a local jar flag only: it is never granted through IPC, so
    /// wire entries carrying it are dropped here (RFC 6265 §5.2.5).
    pub fn apply_to_jar(cookies: &[Self], jar: &mut CookieJar) {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        for cookie in cookies {
            if cookie.http_only
                || cookie.name.is_empty()
                || cookie.domain.is_empty()
                || cookie.domain.starts_with('.')
            {
                continue;
            }
            if let Some(exp) = cookie.expires_epoch_secs
                && exp <= now_secs
            {
                continue;
            }
            let scheme = if cookie.secure { "https" } else { "http" };
            let host = if cookie.domain.contains(':') {
                format!("[{}]", cookie.domain)
            } else {
                cookie.domain.clone()
            };
            let path = if cookie.path.starts_with('/') {
                cookie.path.as_str()
            } else {
                "/"
            };
            let Ok(url) = Url::parse(&format!("{scheme}://{host}{path}")) else {
                continue;
            };
            let mut header = format!("{}={}", cookie.name, cookie.value);
            if !cookie.host_only {
                header.push_str(&format!("; Domain={}", cookie.domain));
            }
            header.push_str(&format!("; Path={path}"));
            if cookie.secure {
                header.push_str("; Secure");
            }
            header.push_str(&format!("; SameSite={}", cookie.same_site.as_header_str()));
            if let Some(exp) = cookie.expires_epoch_secs {
                let remaining = exp.saturating_sub(now_secs).max(1);
                header.push_str(&format!("; Max-Age={remaining}"));
            }
            let _ = jar.store_set_cookie(&url, &header);
        }
    }
}

/// Serializable render options forwarded from parent shell/CLI into child renderer processes.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct IpcRenderOptions {
    pub time_ms: f64,
    pub no_network: bool,
    pub cookies: bool,
    pub seed_cookies: Vec<IpcCookie>,
    pub origin: Option<String>,
    pub max_document_bytes: usize,
    pub max_stylesheets: usize,
    pub max_css_bytes_per_sheet: usize,
    pub max_total_css_bytes: usize,
    pub max_import_depth: usize,
    pub max_images: usize,
    pub max_image_bytes: usize,
    pub max_total_image_bytes: usize,
    pub max_image_dim: Option<u32>,
    pub max_backgrounds: usize,
    pub max_background_bytes: usize,
    pub max_total_background_bytes: usize,
    pub max_total_decoded_pixels: u64,
    pub max_web_fonts: usize,
    pub max_web_font_bytes: usize,
    pub max_total_web_font_bytes: usize,
    pub device_pixel_ratio: f32,
    pub stream_responses: bool,
    pub accept_server_push: bool,
    pub enable_workers: bool,
}

impl Default for IpcRenderOptions {
    fn default() -> Self {
        Self::from_render_options(&RenderOptions::default())
    }
}

impl IpcRenderOptions {
    /// Extracts the serializable option fields and cookie snapshot from `options`.
    pub fn from_render_options(options: &RenderOptions) -> Self {
        // Seed only cookies scoped to the render origin, and never HttpOnly
        // (plan §Task 2; RFC 6265 §5.2.5). Without an origin there is no site
        // to scope against, so no cookie crosses the IPC boundary.
        let seed_cookies = options
            .origin
            .as_deref()
            .and_then(|origin| {
                let guard = options.cookie_jar.as_deref()?.lock().ok()?;
                Some(super::cookie_policy::seeded_cookies_for(
                    &crate::site::site_key(origin),
                    &guard,
                ))
            })
            .unwrap_or_default();
        Self {
            time_ms: options.time_ms,
            no_network: options.no_network,
            cookies: options.cookies || options.cookie_jar.is_some(),
            seed_cookies,
            origin: options.origin.clone(),
            max_document_bytes: options.max_document_bytes,
            max_stylesheets: options.max_stylesheets,
            max_css_bytes_per_sheet: options.max_css_bytes_per_sheet,
            max_total_css_bytes: options.max_total_css_bytes,
            max_import_depth: options.max_import_depth,
            max_images: options.max_images,
            max_image_bytes: options.max_image_bytes,
            max_total_image_bytes: options.max_total_image_bytes,
            max_image_dim: options.max_image_dim,
            max_backgrounds: options.max_backgrounds,
            max_background_bytes: options.max_background_bytes,
            max_total_background_bytes: options.max_total_background_bytes,
            max_total_decoded_pixels: options.max_total_decoded_pixels,
            max_web_fonts: options.max_web_fonts,
            max_web_font_bytes: options.max_web_font_bytes,
            max_total_web_font_bytes: options.max_total_web_font_bytes,
            device_pixel_ratio: options.device_pixel_ratio,
            stream_responses: options.stream_responses,
            accept_server_push: options.accept_server_push,
            enable_workers: options.enable_workers,
        }
    }

    /// Reconstructs child-side [`RenderOptions`], populating a fresh [`CookieJar`] when
    /// cookies are enabled or seeded.
    pub fn to_render_options(&self) -> RenderOptions {
        let cookie_jar = if self.cookies || !self.seed_cookies.is_empty() {
            let mut jar = CookieJar::new();
            IpcCookie::apply_to_jar(&self.seed_cookies, &mut jar);
            Some(Arc::new(Mutex::new(jar)))
        } else {
            None
        };
        RenderOptions {
            time_ms: self.time_ms,
            no_network: self.no_network,
            cookies: self.cookies,
            cookie_jar,
            origin: self.origin.clone(),
            max_document_bytes: self.max_document_bytes,
            max_stylesheets: self.max_stylesheets,
            max_css_bytes_per_sheet: self.max_css_bytes_per_sheet,
            max_total_css_bytes: self.max_total_css_bytes,
            max_import_depth: self.max_import_depth,
            max_images: self.max_images,
            max_image_bytes: self.max_image_bytes,
            max_total_image_bytes: self.max_total_image_bytes,
            max_image_dim: self.max_image_dim,
            max_backgrounds: self.max_backgrounds,
            max_background_bytes: self.max_background_bytes,
            max_total_background_bytes: self.max_total_background_bytes,
            max_total_decoded_pixels: self.max_total_decoded_pixels,
            max_web_fonts: self.max_web_fonts,
            max_web_font_bytes: self.max_web_font_bytes,
            max_total_web_font_bytes: self.max_total_web_font_bytes,
            device_pixel_ratio: self.device_pixel_ratio,
            stream_responses: self.stream_responses,
            accept_server_push: self.accept_server_push,
            enable_workers: self.enable_workers,
            ..RenderOptions::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        IPC_KIND_REQUEST, IPC_KIND_RESPONSE, IpcRenderRequest, IpcRenderResponse, WireHeader,
        read_framed_envelope, run_ipc_worker_stream, run_ipc_worker_stream_with,
        write_framed_envelope,
    };
    use super::*;
    use std::time::Duration;

    const TEST_TOKEN: [u8; 32] = [0x77; 32];

    fn write_request(buffer: &mut Vec<u8>, request: &IpcRenderRequest) {
        write_framed_envelope(
            buffer,
            IPC_KIND_REQUEST,
            request.generation,
            TEST_TOKEN,
            request,
        )
        .expect("write request envelope");
    }

    fn read_response(buffer: &[u8]) -> IpcRenderResponse {
        let (header, response): (WireHeader, IpcRenderResponse) =
            read_framed_envelope(&mut &buffer[..])
                .expect("read response envelope")
                .expect("frame");
        assert_eq!(header.kind, IPC_KIND_RESPONSE);
        assert_eq!(header.site_token, TEST_TOKEN);
        response
    }

    #[test]
    fn ipc_render_options_round_trip_filters_httponly_and_preserves_caps() {
        let mut jar = CookieJar::new();
        let https_url = Url::parse("https://app.example/dashboard").expect("https url");
        assert!(jar.store_set_cookie(
            &https_url,
            "__Host-sid=abc; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=3600"
        ));
        assert!(jar.store_set_cookie(&https_url, "theme=dark; Path=/; Max-Age=3600"));
        let shared = Arc::new(Mutex::new(jar));

        let source = RenderOptions {
            time_ms: 500.0,
            no_network: true,
            cookies: false, // jar alone must still enable IPC cookie forwarding
            cookie_jar: Some(shared),
            origin: Some("https://app.example".to_owned()),
            max_images: 7,
            max_document_bytes: 123_456,
            device_pixel_ratio: 2.0,
            ..RenderOptions::default()
        };

        let wire = IpcRenderOptions::from_render_options(&source);
        let json = serde_json::to_string(&wire).expect("serialize wire options");
        let back: IpcRenderOptions = serde_json::from_str(&json).expect("deserialize wire options");
        let restored = back.to_render_options();

        assert_eq!(restored.time_ms, 500.0);
        assert!(restored.no_network);
        assert!(restored.cookies);
        assert_eq!(restored.origin.as_deref(), Some("https://app.example"));
        assert_eq!(restored.max_images, 7);
        assert_eq!(restored.max_document_bytes, 123_456);
        assert_eq!(restored.device_pixel_ratio, 2.0);
        let restored_jar = restored.cookie_jar.expect("cookie jar restored");
        let guard = restored_jar.lock().expect("lock jar");
        assert_eq!(guard.len(), 1, "HttpOnly cookie must never cross IPC");
        let c = guard.iter().next().expect("stored cookie");
        assert_eq!(c.name, "theme");
        assert_eq!(c.value, "dark");
        assert!(!c.http_only);
        assert!(
            c.expires
                .is_some_and(|e| e > SystemTime::now() + Duration::from_secs(60))
        );
    }

    #[test]
    fn worker_stream_forwards_options_without_returning_cookies() {
        let mut parent_jar = CookieJar::new();
        let origin_url = Url::parse("https://site.test/login").expect("url");
        assert!(parent_jar.store_set_cookie(&origin_url, "prefs=dark; Path=/"));
        let shared = Arc::new(Mutex::new(parent_jar));

        let source = RenderOptions {
            time_ms: 250.0,
            origin: Some("https://site.test".to_owned()),
            cookie_jar: Some(shared),
            max_images: 3,
            ..RenderOptions::default()
        };

        let req = IpcRenderRequest {
            generation: 1,
            input: "data:text/html,<p>ok</p>".to_owned(),
            css_width: 200,
            css_height: 100,
            base_override: None,
            scripting: false,
            find_query: None,
            options: IpcRenderOptions::from_render_options(&source),
        };
        let mut input = Vec::new();
        write_request(&mut input, &req);
        let mut output = Vec::new();
        run_ipc_worker_stream_with(&mut &input[..], &mut output, |req, options| {
            assert_eq!(options.time_ms, 250.0);
            assert_eq!(options.origin.as_deref(), Some("https://site.test"));
            assert_eq!(options.max_images, 3);
            let jar = options.cookie_jar.as_ref().expect("child jar");
            {
                let mut g = jar.lock().expect("lock");
                assert_eq!(g.len(), 1);
                let set_url = Url::parse("https://site.test/api").expect("api url");
                assert!(g.store_set_cookie(&set_url, "token=xyz; Secure"));
            }
            crate::worker::render_document_with_options(req, options)
        })
        .expect("worker stream");

        let response = read_response(&output);
        // The child's Set-Cookie may not flow back over IPC (plan §Task 2).
        let wire = serde_json::to_value(&response).expect("serialize response");
        assert!(wire.get("cookies").is_none());
    }

    #[test]
    fn worker_stream_applies_time_ms_to_keyframes_frame() {
        let source = RenderOptions {
            time_ms: 1000.0,
            ..RenderOptions::default()
        };
        let req = IpcRenderRequest {
            generation: 1,
            input: "data:text/html,<style>@keyframes k{from{background:rgb(255,0,0)}to{background:rgb(0,0,255)}}html,body{margin:0}div{animation:k 1s linear forwards;width:60px;height:60px}</style><div></div>".to_owned(),
            css_width: 60,
            css_height: 60,
            base_override: None,
            scripting: false,
            find_query: None,
            options: IpcRenderOptions::from_render_options(&source),
        };
        let mut input = Vec::new();
        write_request(&mut input, &req);
        let mut output = Vec::new();
        run_ipc_worker_stream(&mut &input[..], &mut output).expect("stream");
        let response = read_response(&output);
        let img = megi_webview::image::decode_png(
            &response.png_bytes,
            &megi_webview::image::PngLimits::default(),
        )
        .expect("png");
        let idx = ((25usize * img.width as usize) + 25usize) * 4;
        assert_eq!(
            [
                img.data[idx],
                img.data[idx + 1],
                img.data[idx + 2],
                img.data[idx + 3]
            ],
            [0, 0, 255, 255]
        );
    }
}
