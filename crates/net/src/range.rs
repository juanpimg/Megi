//! HTTP byte ranges (`Range`/`Content-Range`/`Accept-Ranges`) for resource fetches.
//!
//! Implements the single-range subset of RFC 9110 §14:
//!
//! - [`ByteRange::parse`] accepts `bytes=start-end`, `bytes=start-` and the suffix form
//!   `bytes=-length`; multi-range requests (`bytes=0-1,4-5`) are rejected
//!   ([`RangeError::MultiRange`]) because the response would need
//!   `multipart/byteranges` (documented limitation).
//! - [`ByteRange::resolve`] turns a range into an inclusive `(start, end)` pair for a known
//!   content length, returning `None` for unsatisfiable ranges (the caller can answer
//!   `416`; this client surfaces the server's status verbatim).
//! - [`ContentRange::parse`] parses `Content-Range: bytes 0-499/1234` and the unsatisfied
//!   form `bytes */1234`.
//!
//! [`crate::FetchOptions::range`] makes the HTTP client send the `Range` header;
//! [`crate::Response::content_range`]/[`crate::Response::accept_ranges`] expose the
//! response metadata. The loader has [`crate::ResourceLoader::load_range`] for a one-shot
//! partial fetch (not cached; no `If-Range`, resume or streaming — documented).

use std::fmt;

/// A single HTTP byte range request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByteRange {
    /// `bytes=start-end` (inclusive).
    FromTo {
        /// First byte offset.
        start: u64,
        /// Last byte offset (inclusive), validated against `start`.
        end: u64,
    },
    /// `bytes=start-`: from `start` to the end of the representation.
    From {
        /// First byte offset.
        start: u64,
    },
    /// `bytes=-length`: the last `length` bytes.
    Suffix {
        /// Number of trailing bytes requested.
        length: u64,
    },
}

/// Errors produced by [`ByteRange::parse`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    /// The header does not start with the `bytes=` unit.
    UnsupportedUnit(String),
    /// No byte range was present after `bytes=`.
    Empty,
    /// The range syntax is invalid (`start > end`, non-numeric, ...).
    Invalid(String),
    /// Multiple ranges are not supported (no `multipart/byteranges`).
    MultiRange,
}

impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedUnit(value) => {
                write!(f, "unsupported range unit (expected bytes=): `{value}`")
            }
            Self::Empty => f.write_str("empty byte range"),
            Self::Invalid(value) => write!(f, "invalid byte range `{value}`"),
            Self::MultiRange => f.write_str("multiple byte ranges are not supported"),
        }
    }
}

impl std::error::Error for RangeError {}

impl ByteRange {
    /// Parses a `Range` request header value (`bytes=...`).
    pub fn parse(header: &str) -> Result<Self, RangeError> {
        let header = header.trim();
        let Some(rest) = header
            .strip_prefix("bytes=")
            .or_else(|| header.strip_prefix("Bytes="))
        else {
            return Err(RangeError::UnsupportedUnit(header.to_owned()));
        };
        let rest = rest.trim();
        if rest.is_empty() {
            return Err(RangeError::Empty);
        }
        if rest.contains(',') {
            return Err(RangeError::MultiRange);
        }
        let (raw_start, raw_end) = rest
            .split_once('-')
            .ok_or_else(|| RangeError::Invalid(rest.to_owned()))?;
        let raw_start = raw_start.trim();
        let raw_end = raw_end.trim();
        if raw_start.is_empty() {
            let length: u64 = raw_end
                .parse()
                .map_err(|_| RangeError::Invalid(rest.to_owned()))?;
            if length == 0 {
                return Err(RangeError::Invalid(rest.to_owned()));
            }
            return Ok(Self::Suffix { length });
        }
        let start: u64 = raw_start
            .parse()
            .map_err(|_| RangeError::Invalid(rest.to_owned()))?;
        if raw_end.is_empty() {
            return Ok(Self::From { start });
        }
        let end: u64 = raw_end
            .parse()
            .map_err(|_| RangeError::Invalid(rest.to_owned()))?;
        if end < start {
            return Err(RangeError::Invalid(rest.to_owned()));
        }
        Ok(Self::FromTo { start, end })
    }

    /// Returns the canonical `Range` header value.
    pub fn header_value(&self) -> String {
        match self {
            Self::FromTo { start, end } => format!("bytes={start}-{end}"),
            Self::From { start } => format!("bytes={start}-"),
            Self::Suffix { length } => format!("bytes=-{length}"),
        }
    }

    /// Resolves the range against `content_length`, returning an inclusive byte interval.
    ///
    /// Returns `None` when the range is unsatisfiable for that representation length.
    pub fn resolve(&self, content_length: u64) -> Option<(u64, u64)> {
        if content_length == 0 {
            return None;
        }
        let last = content_length - 1;
        match *self {
            Self::FromTo { start, end } => {
                if start > last {
                    None
                } else {
                    Some((start, end.min(last)))
                }
            }
            Self::From { start } => {
                if start > last {
                    None
                } else {
                    Some((start, last))
                }
            }
            Self::Suffix { length } => {
                if length == 0 {
                    None
                } else {
                    Some((content_length.saturating_sub(length), last))
                }
            }
        }
    }
}

impl fmt::Display for ByteRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.header_value())
    }
}

/// A parsed `Content-Range` response header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContentRange {
    /// First byte offset of the returned slice (0 when `unsatisfied`).
    pub start: u64,
    /// Last byte offset of the returned slice, inclusive (0 when `unsatisfied`).
    pub end: u64,
    /// Complete representation length (`*` in the header when unknown).
    pub total: Option<u64>,
    /// `true` for the `bytes */length` unsatisfied form.
    pub unsatisfied: bool,
}

impl ContentRange {
    /// Parses `Content-Range: bytes start-end/total` or `bytes * /total`.
    pub fn parse(header: &str) -> Option<Self> {
        let rest = header
            .trim()
            .strip_prefix("bytes")
            .or_else(|| header.trim().strip_prefix("Bytes"))?
            .trim();
        let (range_part, total_part) = rest.split_once('/')?;
        let total = total_part.trim();
        let total = if total == "*" {
            None
        } else {
            Some(total.parse::<u64>().ok()?)
        };
        let range_part = range_part.trim();
        if range_part == "*" {
            return Some(Self {
                start: 0,
                end: 0,
                total,
                unsatisfied: true,
            });
        }
        let (raw_start, raw_end) = range_part.split_once('-')?;
        let start: u64 = raw_start.trim().parse().ok()?;
        let end: u64 = raw_end.trim().parse().ok()?;
        if end < start || total.is_some_and(|total| end >= total) {
            return None;
        }
        Some(Self {
            start,
            end,
            total,
            unsatisfied: false,
        })
    }

    /// Number of bytes in the returned slice (0 for unsatisfied ranges).
    pub const fn length(&self) -> u64 {
        if self.unsatisfied {
            0
        } else {
            self.end - self.start + 1
        }
    }
}

impl fmt::Display for ContentRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let total = self
            .total
            .map_or_else(|| "*".to_owned(), |total| total.to_string());
        if self.unsatisfied {
            write!(f, "bytes */{total}")
        } else {
            write!(f, "bytes {}-{}/{total}", self.start, self.end)
        }
    }
}
