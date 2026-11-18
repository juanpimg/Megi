use super::*;
use crate::error::FetchError;
use std::io::Cursor;

fn read_one(bytes: &[u8]) -> Result<Option<Frame>, FetchError> {
    FrameReader::new(Cursor::new(bytes.to_vec()), DEFAULT_MAX_FRAME_SIZE).read_frame()
}

#[test]
fn frame_roundtrip() {
    let frames = [
        Frame::settings(&[(settings::MAX_FRAME_SIZE, 16_384)]),
        Frame::settings_ack(),
        Frame::window_update(0, 1_000_000),
        Frame::ping([1, 2, 3, 4, 5, 6, 7, 8], false),
        Frame::headers(1, &[0x82, 0x86], true),
        Frame::data(1, b"hello", true),
        Frame::rst_stream(1, error_code::CANCEL),
        Frame::goaway(1, error_code::NO_ERROR, b"bye"),
    ];
    let mut bytes = Vec::new();
    for frame in &frames {
        frame.write_to(&mut bytes);
    }
    let mut reader = FrameReader::new(Cursor::new(bytes), DEFAULT_MAX_FRAME_SIZE);
    for expected in &frames {
        let frame = reader.read_frame().expect("valid frame").expect("not EOF");
        assert_eq!(&frame, expected);
    }
    assert!(reader.read_frame().expect("clean EOF").is_none());
}

#[test]
fn assembles_continuation_headers() {
    let mut assembler = HeaderAssembler::default();
    let headers = Frame::new(kind::HEADERS, 0, 1, vec![1, 2]);
    assert!(assembler.feed(&headers).expect("headers").is_none());
    let continuation = Frame::continuation(1, &[3, 4], false);
    assert!(assembler.feed(&continuation).expect("cont").is_none());
    let last = Frame::continuation(1, &[5], true);
    let (block, end_stream) = assembler
        .feed(&last)
        .expect("complete block")
        .expect("end headers");
    assert_eq!(block, vec![1, 2, 3, 4, 5]);
    assert!(!end_stream);
}

#[test]
fn rejects_truncated_and_oversized_frames() {
    let frame = Frame::data(1, &[0u8; 32], false);
    let mut bytes = Vec::new();
    frame.write_to(&mut bytes);
    assert!(read_one(&bytes[..bytes.len() - 5]).is_err());
    let mut oversized = Vec::new();
    Frame::new(kind::DATA, 0, 1, vec![0u8; 20_000]).write_to(&mut oversized);
    let error = read_one(&oversized).expect_err("oversized frame");
    assert!(matches!(error, FetchError::Http2(_)));
}

#[test]
fn rejects_invalid_stream_ids_and_lengths() {
    let wrong_stream = Frame::new(kind::DATA, 0, 0, vec![1]);
    let mut bytes = Vec::new();
    wrong_stream.write_to(&mut bytes);
    assert!(read_one(&bytes).is_err());

    let bad_ping = Frame::new(kind::PING, 0, 0, vec![0; 7]);
    let mut bytes = Vec::new();
    bad_ping.write_to(&mut bytes);
    assert!(read_one(&bytes).is_err());

    let bad_settings = Frame::new(kind::SETTINGS, 0, 0, vec![0; 5]);
    let mut bytes = Vec::new();
    bad_settings.write_to(&mut bytes);
    assert!(read_one(&bytes).is_err());
}

#[test]
fn stream_state_transitions() {
    assert_eq!(StreamState::Open.local_end(), StreamState::HalfClosedLocal);
    assert_eq!(
        StreamState::HalfClosedLocal.remote_end(),
        StreamState::Closed
    );
    assert_eq!(
        StreamState::Idle.remote_end(),
        StreamState::HalfClosedRemote
    );
}
