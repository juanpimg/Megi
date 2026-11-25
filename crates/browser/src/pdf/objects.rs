//! Minimal indirect-object writer for the PDF writer.
//!
//! ISO 32000-1 §7.3 defines the `n g obj … endobj` syntax; §7.5.8 defines the
//! classic cross-reference table that maps object numbers to byte offsets.
//! Keeping the writer here lets the text layer share one deterministic xref.

/// Accumulates a PDF 1.4 file and the byte offset of every indirect object.
pub(super) struct PdfWriter {
    out: Vec<u8>,
    offsets: Vec<usize>,
}

impl PdfWriter {
    /// Starts the file with the PDF 1.4 header (§7.5.2).
    pub(super) fn new(capacity: usize) -> Self {
        let mut out = Vec::with_capacity(capacity);
        out.extend_from_slice(b"%PDF-1.4\n");
        Self {
            out,
            offsets: Vec::with_capacity(8),
        }
    }

    /// Appends an object body and returns its 1-based object number.
    pub(super) fn object(&mut self, body: &[u8]) -> u32 {
        self.offsets.push(self.out.len());
        let number = self.offsets.len() as u32;
        self.out
            .extend_from_slice(format!("{number} 0 obj\n").as_bytes());
        self.out.extend_from_slice(body);
        self.out.extend_from_slice(b"\nendobj\n");
        number
    }

    /// Appends a stream object; `/Length` counts the stream bytes, not the dictionary.
    pub(super) fn stream(&mut self, dict: &str, data: &[u8]) -> u32 {
        let mut body = format!("{dict}\nstream\n").into_bytes();
        body.extend_from_slice(data);
        body.extend_from_slice(b"\nendstream");
        self.object(&body)
    }

    /// Appends the §7.5.8 xref + trailer and returns the complete file.
    pub(super) fn finish(mut self) -> Vec<u8> {
        let xref_offset = self.out.len();
        let size = self.offsets.len() + 1;
        self.out
            .extend_from_slice(format!("xref\n0 {size}\n").as_bytes());
        self.out.extend_from_slice(b"0000000000 65535 f \n");
        for offset in &self.offsets {
            self.out
                .extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        self.out.extend_from_slice(
            format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n")
                .as_bytes(),
        );
        self.out
    }
}
