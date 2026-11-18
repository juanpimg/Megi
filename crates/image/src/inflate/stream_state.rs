#[allow(unused_imports)]
use super::*;

/// Mode of a [`StreamingInflater`] container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StreamMode {
    Zlib,
    Gzip,
    RawDeflate,
    /// HTTP `deflate`: zlib if the first two bytes validate, otherwise raw DEFLATE.
    AutoDeflate,
}

/// Input bit/byte cursor shared by the streaming state machine.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct StreamPosition {
    pub(crate) byte: usize,
    pub(crate) bit: u8,
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)] // Huffman tables are large but each inflater has one state.
pub(crate) enum StreamState {
    Start,
    GzipStart,
    BlockHeader,
    Stored {
        remaining: usize,
        final_block: bool,
    },
    Compressed {
        literals: Huffman,
        distances: Huffman,
        final_block: bool,
    },
    Trailer,
    Done,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum StreamChecksum {
    None,
    Adler { a: u32, b: u32 },
    Gzip { crc: u32, isize: u32 },
}

impl StreamChecksum {
    pub fn new(mode: StreamMode) -> Self {
        match mode {
            StreamMode::Zlib | StreamMode::AutoDeflate => Self::Adler { a: 1, b: 0 },
            StreamMode::Gzip => Self::Gzip {
                crc: 0xFFFF_FFFF,
                isize: 0,
            },
            StreamMode::RawDeflate => Self::None,
        }
    }

    pub(crate) fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::None => {}
            Self::Adler { a, b } => {
                const MOD: u32 = 65_521;
                for chunk in bytes.chunks(1_024) {
                    for &byte in chunk {
                        *a += u32::from(byte);
                        *b += *a;
                    }
                    *a %= MOD;
                    *b %= MOD;
                }
            }
            Self::Gzip { crc, isize } => {
                *crc = crate::crc32::crc32_update(*crc, bytes);
                *isize = isize.wrapping_add(bytes.len() as u32);
            }
        }
    }
}

/// Incremental zlib/DEFLATE inflater for streaming consumers (image/net).
///
/// Feed compressed chunks with [`StreamingInflater::feed`]; each call appends the bytes
/// decompressed from that chunk to the supplied output buffer (possibly several callbacks
/// worth). Call [`StreamingInflater::finish`] at end-of-stream to validate the zlib
/// Adler-32 / gzip CRC-32+ISIZE trailer. The implementation reuses the same Huffman
/// tables/decoder as the batch inflater; state is retained across feeds, including
/// straddling block headers, dynamic tables and symbols.
///
/// `AutoDeflate` accepts the HTTP `deflate` coding in either its zlib (RFC 1950) or raw
/// DEFLATE (RFC 1951) form, like [`inflate_deflate`] / `decode_deflate` in `net`.
#[derive(Clone, Debug)]
pub struct StreamingInflater {
    pub(crate) mode: StreamMode,
    pub(crate) limits: InflateLimits,
    pub(crate) input: Vec<u8>,
    pub(crate) position: StreamPosition,
    pub(crate) state: StreamState,
    pub(crate) checksum: StreamChecksum,
    pub(crate) failed: Option<InflateError>,
    pub(crate) total_output: usize,
    /// Last up-to-32 KiB of decompressed data, kept for LZ77 back-references that
    /// straddle feed boundaries.
    pub(crate) window: Vec<u8>,
}

impl StreamingInflater {
    /// Creates an inflater for a zlib (RFC 1950) stream.
    pub fn new_zlib(limits: InflateLimits) -> Self {
        Self::new_with_mode(StreamMode::Zlib, limits)
    }

    /// Creates an inflater for a gzip (RFC 1952) member.
    pub fn new_gzip(limits: InflateLimits) -> Self {
        Self::new_with_mode(StreamMode::Gzip, limits)
    }

    /// Creates an inflater for a raw DEFLATE (RFC 1951) stream.
    pub fn new_raw_deflate(limits: InflateLimits) -> Self {
        Self::new_with_mode(StreamMode::RawDeflate, limits)
    }

    /// Creates an inflater for HTTP `deflate`: zlib or raw DEFLATE auto-detected.
    pub fn new_auto_deflate(limits: InflateLimits) -> Self {
        Self::new_with_mode(StreamMode::AutoDeflate, limits)
    }

    pub(crate) fn new_with_mode(mode: StreamMode, limits: InflateLimits) -> Self {
        let checksum = StreamChecksum::new(mode);
        let state = match mode {
            StreamMode::Zlib | StreamMode::AutoDeflate => StreamState::Start,
            StreamMode::Gzip => StreamState::GzipStart,
            StreamMode::RawDeflate => StreamState::BlockHeader,
        };
        Self {
            mode,
            limits,
            input: Vec::new(),
            position: StreamPosition::default(),
            state,
            checksum,
            failed: None,
            total_output: 0,
            window: Vec::new(),
        }
    }

    /// Feeds a compressed chunk, appending decompressed bytes to `output`.
    ///
    /// A malformed stream returns a typed [`InflateError`]; the inflater becomes poisoned
    /// and later calls return the same error. Truncated input is *not* an error here: it
    /// simply waits for more bytes (call [`Self::finish`] to reject an incomplete stream).
    pub fn feed(&mut self, input: &[u8], output: &mut Vec<u8>) -> Result<(), InflateError> {
        if let Some(error) = self.failed {
            return Err(error);
        }
        if matches!(self.state, StreamState::Done) {
            // Trailing bytes after the trailer are ignored (PNG/net single-member contract).
            return Ok(());
        }
        self.input.extend_from_slice(input);
        // Auto-detect HTTP `deflate` before the state machine starts: valid zlib header
        // (method 8, window <=7, FCHECK) selects zlib; anything else is raw DEFLATE.
        if self.mode == StreamMode::AutoDeflate && matches!(self.state, StreamState::Start) {
            if self.input.len() >= 2 {
                let cmf = self.input[0];
                let flg = self.input[1];
                let zlib = cmf & 0x0F == 8
                    && cmf >> 4 <= 7
                    && (((cmf as u16) << 8) | flg as u16).is_multiple_of(31);
                if zlib {
                    self.mode = StreamMode::Zlib;
                } else {
                    self.mode = StreamMode::RawDeflate;
                    self.state = StreamState::BlockHeader;
                }
            } else {
                return Ok(());
            }
        }
        loop {
            match drive(
                &self.input,
                &mut self.position,
                &mut self.state,
                self.mode,
                &mut self.checksum,
                output,
                &mut self.window,
                self.limits,
                &mut self.total_output,
            ) {
                Ok(true) => continue,
                Ok(false) => break,
                Err(error) => {
                    self.failed = Some(error);
                    return Err(error);
                }
            }
        }
        // Drain consumed compressed bytes to avoid pinning the full input.
        if self.position.bit == 0 && self.position.byte > 0 {
            let consumed = self.position.byte.min(self.input.len());
            self.input.drain(..consumed);
            self.position.byte -= consumed;
        }
        Ok(())
    }

    /// Completes the stream: verifies the trailer and rejects truncated input.
    pub fn finish(&mut self) -> Result<(), InflateError> {
        if let Some(error) = self.failed {
            return Err(error);
        }
        if matches!(self.state, StreamState::Done) {
            return Ok(());
        }
        // Drive until done or truncated; loop instead of recursing (#505).
        let mut scratch = Vec::new();
        loop {
            match drive(
                &self.input,
                &mut self.position,
                &mut self.state,
                self.mode,
                &mut self.checksum,
                &mut scratch,
                &mut self.window,
                self.limits,
                &mut self.total_output,
            ) {
                Ok(true) => {
                    if matches!(self.state, StreamState::Done) {
                        return Ok(());
                    }
                }
                Ok(false) => {
                    let error = InflateError::TruncatedInput;
                    self.failed = Some(error);
                    return Err(error);
                }
                Err(err) => {
                    self.failed = Some(err);
                    return Err(err);
                }
            }
        }
    }

    /// Returns the total number of decompressed bytes produced so far.
    pub fn output_len(&self) -> usize {
        self.total_output
    }

    /// Returns `true` once the trailer was validated.
    pub fn is_done(&self) -> bool {
        matches!(self.state, StreamState::Done)
    }
}
