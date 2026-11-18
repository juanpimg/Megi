#[allow(unused_imports)]
use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub(crate) fn adler32_known_vectors() {
        assert_eq!(adler32(b""), 1);
        assert_eq!(adler32(b"a"), 0x0062_0062);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    pub(crate) fn inflates_an_empty_zlib_stream() {
        // zlib.compress(b"") from CPython.
        let empty = [0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01];
        assert_eq!(inflate_zlib(&empty, InflateLimits::default()).unwrap(), b"");
    }

    #[test]
    pub(crate) fn rejects_reserved_block_type() {
        // BFINAL=1, BTYPE=11.
        let raw = [0x07u8];
        assert_eq!(
            inflate_deflate(&raw, InflateLimits::default()),
            Err(InflateError::InvalidBlockType)
        );
    }

    #[test]
    pub(crate) fn rejects_truncated_and_empty_input() {
        assert_eq!(
            inflate_zlib(&[], InflateLimits::default()),
            Err(InflateError::EmptyInput)
        );
        assert_eq!(
            inflate_zlib(&[0x78], InflateLimits::default()),
            Err(InflateError::TruncatedInput)
        );
        assert_eq!(
            inflate_deflate(&[0x01], InflateLimits::default()),
            Err(InflateError::TruncatedInput)
        );
    }

    #[test]
    pub(crate) fn rejects_bad_zlib_header() {
        assert_eq!(
            inflate_zlib(
                &[0x79, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01],
                InflateLimits::default()
            ),
            Err(InflateError::UnsupportedZlibMethod)
        );
        assert_eq!(
            inflate_zlib(
                &[0x78, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01],
                InflateLimits::default()
            ),
            Err(InflateError::InvalidZlibHeader)
        );
        // FDICT set (0x78 0xbb is a valid FCHECK combination with FDICT).
        assert_eq!(
            inflate_zlib(
                &[0x78, 0xbb, 0x00, 0x00, 0x00, 0x00],
                InflateLimits::default()
            ),
            Err(InflateError::UnsupportedPresetDictionary)
        );
    }

    #[test]
    pub(crate) fn enforces_the_output_limit() {
        let stored = [
            0x78, 0x01, // zlib header
            0x01, 0x05, 0x00, 0xfa, 0xff, // stored block: BFINAL+LEN/NLEN
            b'a', b'b', b'c', b'd', b'e', // payload
            0x05, 0xc8, 0x01, 0xf0, // Adler-32("abcde")
        ];
        assert_eq!(
            inflate_zlib(&stored, InflateLimits::new(4)),
            Err(InflateError::OutputLimitExceeded { limit: 4 })
        );
        assert_eq!(
            inflate_zlib(&stored, InflateLimits::new(5)).unwrap(),
            b"abcde"
        );
    }

    #[test]
    pub(crate) fn rejects_adler_mismatch() {
        let mut corrupted = [0x78, 0x01, 0x01, 0x01, 0x00, 0xfe, 0xff, b'x', 0, 0, 0, 0];
        // Valid adler of "x" is 0x00790079.
        corrupted[8..12].copy_from_slice(&0x0079_0079u32.to_be_bytes());
        assert!(inflate_zlib(&corrupted, InflateLimits::default()).is_ok());
        corrupted[11] ^= 0xFF;
        assert!(matches!(
            inflate_zlib(&corrupted, InflateLimits::default()),
            Err(InflateError::AdlerMismatch { .. })
        ));
    }
}
