//! Helpers shared by the loader impl blocks: range slicing and cache-entry updates.

use super::*;

/// Slices `bytes` with `range`, mapping unsatisfiable ranges to [`FetchError::InvalidRange`].
pub(super) fn slice_bytes(bytes: &[u8], range: ByteRange) -> Result<Vec<u8>, FetchError> {
    let Some((start, end)) = range.resolve(bytes.len() as u64) else {
        return Err(FetchError::InvalidRange(format!(
            "range {range} is unsatisfiable for {} bytes",
            bytes.len()
        )));
    };
    Ok(bytes[start as usize..=end as usize].to_vec())
}

/// Builds a [`Resource`] from an HTTP response, resolving its content type.
pub(super) fn resource_from_response(response: Response) -> Resource {
    let content_type = mime::resolve_content_type(&response.headers, &response.body);
    Resource {
        status: response.status,
        url: response.final_url,
        bytes: response.body,
        content_type,
    }
}

/// Recomputes a cache entry after a `304 Not Modified`, keeping the stored body.
pub(super) fn updated_entry(
    mut entry: CachedEntry,
    policy: &CachePolicy,
    user_agent: String,
) -> CachedEntry {
    if policy.no_store {
        // A `304` with `no-store` invalidates the stored representation.
        entry.freshness = Duration::ZERO;
        entry.no_cache = true;
    } else {
        let freshness = policy
            .freshness_lifetime(SystemTime::now())
            .unwrap_or(HEURISTIC_FRESHNESS);
        entry.freshness = freshness;
        entry.no_cache = policy.no_cache;
        if policy.etag.is_some() {
            entry.etag = policy.etag.clone();
        }
        if policy.last_modified.is_some() {
            entry.last_modified = policy.last_modified.clone();
        }
    }
    entry.stored_at = Instant::now();
    entry.vary_user_agent = if policy.vary.user_agent {
        Some(user_agent)
    } else {
        None
    };
    entry
}
