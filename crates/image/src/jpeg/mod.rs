//! Baseline JPEG (ITU-T T.81 / JFIF) decoder — `std` only, no `unsafe`.
//!
//! ## Supported subset
//!
//! - **Baseline sequential DCT** (`SOF0`) and **progressive DCT (`SOF2`)**:
//!   DC-first/DC-refinement, AC-first and AC-refinement scans; spectral selection
//!   and successive approximation are decoded into coefficient buffers and IDCT'd
//!   once at EOI. Multi-component (colour) progressive images decode fully, including
//!   interleaved DC-refinement scans (`Ss == 0`, `Ah > 0`, `Ns > 1`) (#1661); only
//!   interleaved **AC** scans (`Ss > 0` with more than one component) are rejected
//!   with a clear [`ImageError::UnsupportedFeature`]; grayscale progressive decodes fully;
//! - Huffman entropy coding (`DHT`/`SOS`);
//! - quantization tables (`DQT`, 8- and 16-bit precision), 1-4 components;
//! - sampling factors **4:4:4, 4:2:2 and 4:2:0** (any `H/V` 1-4 within the MCU limits);
//! - restart intervals (`DRI` + `RSTn` markers, DC predictor reset);
//! - grayscale (`1` component) and `YCbCr` (3 components); Adobe `APP14` transform `0`
//!   (RGB) is honoured, `CMYK`/`YCCK` are rejected with a clear error;
//! - nearest-neighbour chroma upsampling (4:4:4 is exact; 4:2:2/4:2:0 are documented as
//!   blocky chroma — acceptable for photographs at the engine's current fidelity);
//! - EXIF `APP1`/JFIF `APP0` are skipped by the JPEG decoder itself; **EXIF orientation**
//!   (`IFD0` tag `0x0112`) is parsed by [`crate::exif`] and applied by [`crate::decode`]
//!   after decoding (all eight orientations).
//!
//! ## Rejected with a clear error (never a panic)
//!
//! - progressive **interleaved AC scans** (`Ss > 0`, `Ns > 1`, documented above),
//!   arithmetic/lossless/hierarchical DCT modes;
//! - multiple scans: only the first `SOS` scan is decoded (baseline files encode
//!   everything in one interleaved scan; extra scans are ignored, documented);
//! - `CMYK`/`YCCK` (4-component Adobe) and 12-bit sample precision.
//!
//! Malformed/truncated streams return a typed [`ImageError`]; dimensions are checked against
//! [`PngLimits`] before any pixel allocation, so a hostile header cannot allocate unbounded
//! memory. Missing/extra entropy bits are tolerated only where the spec allows padding.

use crate::RgbaImage;
use crate::error::ImageError;
use crate::png::PngLimits;

mod constants;
mod decode;
mod frame;
mod huffman;
mod markers;
mod progressive;
mod scan;
mod store;
mod tests;

#[allow(unused_imports)]
pub(crate) use constants::*;
#[allow(unused_imports)]
pub(crate) use decode::*;
#[allow(unused_imports)]
pub(crate) use frame::*;
#[allow(unused_imports)]
pub(crate) use huffman::*;
#[allow(unused_imports)]
pub(crate) use markers::*;
#[allow(unused_imports)]
pub(crate) use progressive::*;
#[allow(unused_imports)]
pub(crate) use scan::*;
#[allow(unused_imports)]
pub(crate) use store::*;
#[allow(unused_imports)]
pub(crate) use tests::*;
