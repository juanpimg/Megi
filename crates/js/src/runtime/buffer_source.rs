use boa_engine::{
    Context, JsObject, JsResult, JsValue,
    object::builtins::{
        JsArrayBuffer, JsDataView, JsSharedArrayBuffer, JsTypedArray, JsUint8Array,
        JsUint8ClampedArray,
    },
};

/// Copies `[offset..offset + len]` bytes from an `ArrayBuffer` or `SharedArrayBuffer`.
pub(crate) fn slice_backing_buffer(buffer: &JsValue, offset: usize, len: usize) -> Option<Vec<u8>> {
    let buffer_obj = buffer.as_object()?;
    let end = offset.saturating_add(len);
    if let Ok(ab) = JsArrayBuffer::from_object(buffer_obj.clone()) {
        return Some(
            ab.data()
                .and_then(|bytes| bytes.get(offset..end).map(|slice| slice.to_vec()))
                .unwrap_or_default(),
        );
    }
    if let Ok(sab) = JsSharedArrayBuffer::from_object(buffer_obj) {
        let all = sab.to_vec();
        return Some(
            all.get(offset..end)
                .map(|slice| slice.to_vec())
                .unwrap_or_default(),
        );
    }
    None
}

/// Extracts raw bytes when `object` is a WebIDL `BufferSource` (`ArrayBuffer`, `TypedArray`,
/// `DataView`, or `SharedArrayBuffer`).
pub(crate) fn extract_buffer_source_bytes(
    object: &JsObject,
    context: &mut Context,
) -> JsResult<Option<Vec<u8>>> {
    if let Ok(ab) = JsArrayBuffer::from_object(object.clone()) {
        return Ok(Some(
            ab.data().map(|bytes| bytes.to_vec()).unwrap_or_default(),
        ));
    }
    if let Ok(sab) = JsSharedArrayBuffer::from_object(object.clone()) {
        return Ok(Some(sab.to_vec()));
    }
    if let Ok(ta) = JsTypedArray::from_object(object.clone()) {
        let buffer = ta.buffer(context)?;
        let offset = ta.byte_offset(context)?;
        let len = ta.byte_length(context)?;
        return Ok(slice_backing_buffer(&buffer, offset, len));
    }
    if let Ok(dv) = JsDataView::from_object(object.clone()) {
        let buffer = dv.buffer(context)?;
        let offset = dv.byte_offset(context)? as usize;
        let len = dv.byte_length(context)? as usize;
        return Ok(slice_backing_buffer(&buffer, offset, len));
    }
    Ok(None)
}

/// Copies up to `max_len` bytes directly from a `Uint8ClampedArray` or `Uint8Array` backing
/// buffer in linear time without per-index JS property lookups (#2183).
pub(crate) fn extract_u8_typed_array_bytes(
    object: &JsObject,
    max_len: usize,
    context: &mut Context,
) -> Option<Vec<u8>> {
    let ta: JsTypedArray = if let Ok(arr) = JsUint8ClampedArray::from_object(object.clone()) {
        (*arr).clone()
    } else if let Ok(arr) = JsUint8Array::from_object(object.clone()) {
        (*arr).clone()
    } else {
        return None;
    };
    let buffer = ta.buffer(context).ok()?;
    let offset = ta.byte_offset(context).ok()?;
    let byte_len = ta.byte_length(context).ok()?;
    slice_backing_buffer(&buffer, offset, max_len.min(byte_len))
}
