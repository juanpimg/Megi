//! Generic vertex attribute arrays for the software WebGL path (§5.9).
//!
//! Split from `canvas/webgl.rs` to keep that file inside the size budget. The
//! state is owned by `CanvasSurface`; no JS handle or borrow crosses a crate.

use super::{
    FLOAT, INVALID_ENUM, INVALID_VALUE, MAX_BUFFER_BYTES, MAX_VERTEX_ATTRIBS, WebGLBuffer,
};

/// WebGL 1.0 §5.9: one generic vertex attribute array. The software path reads
/// little-endian `FLOAT` pairs from `buffer` at `offset + index * stride`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexAttribPointer {
    pub index: u32,
    pub size: u32,
    pub kind: u32,
    pub normalized: bool,
    pub stride: usize,
    pub offset: usize,
    /// Index into `WebGLState::buffers` captured when the pointer was set.
    pub buffer: usize,
}

impl VertexAttribPointer {
    /// WebGL 1.0 §5.9: validates WebIDL-converted arguments and captures the
    /// currently bound buffer. Only `FLOAT` components are readable.
    pub fn new(
        index: u32,
        size: u32,
        kind: u32,
        normalized: bool,
        stride: usize,
        offset: usize,
        buffer: usize,
    ) -> Result<Self, u32> {
        if index as usize >= MAX_VERTEX_ATTRIBS || !(1..=4).contains(&size) {
            return Err(INVALID_VALUE);
        }
        if kind != FLOAT {
            return Err(INVALID_ENUM);
        }
        if stride > MAX_BUFFER_BYTES || offset > MAX_BUFFER_BYTES {
            return Err(INVALID_VALUE);
        }
        Ok(Self {
            index,
            size,
            kind,
            normalized,
            stride,
            offset,
            buffer,
        })
    }

    /// Number of complete vertices readable from `buffer`.
    pub fn vertex_count(&self, buffer: &WebGLBuffer) -> usize {
        if buffer.deleted {
            return 0;
        }
        let element = self.size as usize * 4;
        let stride = if self.stride == 0 {
            element
        } else {
            self.stride
        };
        let available = buffer.bytes.len().saturating_sub(self.offset);
        if available < element {
            return 0;
        }
        1 + (available - element) / stride
    }

    /// Reads one `FLOAT` attribute pair as little-endian clip-space `(x, y)`.
    /// A one-component attribute has an implicit `y` of zero (§5.9).
    pub fn vertex(&self, buffer: &WebGLBuffer, index: usize) -> Option<(f32, f32)> {
        if buffer.deleted {
            return None;
        }
        let element = self.size as usize * 4;
        let stride = if self.stride == 0 {
            element
        } else {
            self.stride
        };
        let start = self.offset.checked_add(index.checked_mul(stride)?)?;
        let bytes = buffer.bytes.get(start..start.checked_add(element)?)?;
        let x = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let y = if self.size >= 2 {
            f32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]])
        } else {
            0.0
        };
        Some((x, y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(bytes: Vec<u8>) -> WebGLBuffer {
        WebGLBuffer {
            handle: 1,
            bytes,
            deleted: false,
        }
    }

    #[test]
    fn attrib_pointer_reads_strided_float_pairs() {
        let mut bytes = Vec::new();
        for pair in [(-0.5_f32, 0.25_f32), (0.75, -1.0)] {
            bytes.extend_from_slice(&pair.0.to_le_bytes());
            bytes.extend_from_slice(&pair.1.to_le_bytes());
            bytes.extend_from_slice(&0.0_f32.to_le_bytes());
        }
        let pointer = VertexAttribPointer::new(0, 2, FLOAT, false, 12, 0, 0).unwrap();
        let buffer = buffer(bytes);
        assert_eq!(pointer.vertex_count(&buffer), 2);
        assert_eq!(pointer.vertex(&buffer, 1), Some((0.75, -1.0)));
    }

    #[test]
    fn attrib_pointer_rejects_bad_type_and_index() {
        assert_eq!(
            VertexAttribPointer::new(0, 2, 0x1400, false, 0, 0, 0),
            Err(INVALID_ENUM)
        );
        assert_eq!(
            VertexAttribPointer::new(MAX_VERTEX_ATTRIBS as u32, 2, FLOAT, false, 0, 0, 0),
            Err(INVALID_VALUE)
        );
    }
}
