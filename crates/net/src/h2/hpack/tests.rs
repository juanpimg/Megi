use super::*;

#[test]
fn decodes_rfc7541_c4_1_first_request() {
    // RFC 7541 C.4.1: GET http://www.example.com/ with Huffman-coded authority.
    let block = [
        0x82, 0x86, 0x84, 0x41, 0x8c, 0xf1, 0xe3, 0xc2, 0xe5, 0xf2, 0x3a, 0x6b, 0xa0, 0xab, 0x90,
        0xf4, 0xff,
    ];
    let headers = Decoder::default().decode(&block).expect("valid HPACK");
    assert_eq!(
        headers,
        vec![
            (":method".to_owned(), "GET".to_owned()),
            (":scheme".to_owned(), "http".to_owned()),
            (":path".to_owned(), "/".to_owned()),
            (":authority".to_owned(), "www.example.com".to_owned()),
        ]
    );
}

#[test]
fn decodes_rfc7541_c4_2_second_request_with_dynamic_table() {
    let first = [
        0x82, 0x86, 0x84, 0x41, 0x8c, 0xf1, 0xe3, 0xc2, 0xe5, 0xf2, 0x3a, 0x6b, 0xa0, 0xab, 0x90,
        0xf4, 0xff,
    ];
    let second = [
        0x82, 0x86, 0x84, 0xbe, 0x58, 0x86, 0xa8, 0xeb, 0x10, 0x64, 0x9c, 0xbf,
    ];
    let mut decoder = Decoder::default();
    decoder.decode(&first).expect("first block");
    let headers = decoder.decode(&second).expect("second block");
    assert_eq!(
        headers,
        vec![
            (":method".to_owned(), "GET".to_owned()),
            (":scheme".to_owned(), "http".to_owned()),
            (":path".to_owned(), "/".to_owned()),
            (":authority".to_owned(), "www.example.com".to_owned()),
            ("cache-control".to_owned(), "no-cache".to_owned()),
        ]
    );
    // After C.4.2 the dynamic table holds :authority (57 B) and
    // cache-control: no-cache (53 B): 110 B total.
    assert_eq!(decoder.table_size(), 110);
}

#[test]
fn rejects_a_table_size_update_above_the_advertised_maximum() {
    let mut decoder = Decoder::new(4096);
    // Dynamic table size update to 8192 (5-bit prefix 31 + 7-bit continuation).
    let error = decoder
        .decode(&[0x3f, 0xe1, 0x3f])
        .expect_err("update above the advertised cap");
    assert!(
        error.to_string().contains("exceeds the advertised maximum"),
        "{error}"
    );
    // Shrinking the table is valid (2048).
    decoder
        .decode(&[0x3f, 0xe1, 0x0f])
        .expect("2048 is allowed");
}

#[test]
fn caps_the_decoded_header_list() {
    // One 3001-byte value with incremental indexing (it fits the 4096-byte table)...
    let mut block = vec![0x40, 0x01, b'a', 0x7f, 0xb9, 0x16];
    block.extend(std::iter::repeat_n(b'a', 3000));
    // ... then index it repeatedly: 100 * (3001 + 32) exceeds the 256 KiB cap.
    block.extend(std::iter::repeat_n(0xbe, 100));
    let mut decoder = Decoder::new(4096);
    let error = decoder.decode(&block).expect_err("header list cap");
    assert!(error.to_string().contains("decoded header list"), "{error}");

    // The same block under the cap decodes fine.
    let mut small = vec![0x40, 0x01, b'a', 0x7f, 0xb9, 0x16];
    small.extend(std::iter::repeat_n(b'a', 3000));
    small.extend(std::iter::repeat_n(0xbe, 10));
    let mut decoder = Decoder::new(4096);
    assert_eq!(decoder.decode(&small).expect("under the cap").len(), 11);
}

#[test]
fn allows_a_table_size_update_at_the_start_of_a_later_block() {
    let mut decoder = Decoder::new(4096);
    decoder
        .decode(&[0x82])
        .expect("first block: indexed :method GET");
    decoder
        .decode(&[0x3f, 0xe1, 0x0f])
        .expect("a size update at the start of the next block is valid");
}

#[test]
fn encoder_decoder_roundtrip() {
    let headers = vec![
        (":method".to_owned(), "GET".to_owned()),
        (":scheme".to_owned(), "https".to_owned()),
        (":authority".to_owned(), "example.com".to_owned()),
        (":path".to_owned(), "/index.html".to_owned()),
        ("accept".to_owned(), "*/*".to_owned()),
        ("user-agent".to_owned(), "megi-test/1.0".to_owned()),
    ];
    let block = Encoder::new().encode(&headers);
    let decoded = Decoder::default().decode(&block).expect("roundtrip");
    assert_eq!(decoded, headers);
}

#[test]
fn huffman_known_strings() {
    assert_eq!(
        huffman_decode(&[
            0xf1, 0xe3, 0xc2, 0xe5, 0xf2, 0x3a, 0x6b, 0xa0, 0xab, 0x90, 0xf4, 0xff
        ])
        .unwrap(),
        b"www.example.com"
    );
    assert_eq!(
        huffman_decode(&[0xa8, 0xeb, 0x10, 0x64, 0x9c, 0xbf]).unwrap(),
        b"no-cache"
    );
}

#[test]
fn huffman_rejects_eos_truncation_and_bad_padding() {
    // EOS (30 one bits) is not a valid symbol in a string.
    assert!(huffman_decode(&[0xff, 0xff, 0xff, 0xff]).is_err());
    // A lone high prefix that never completes a code and is not all-ones padding.
    assert!(huffman_decode(&[0x00]).is_err());
}

#[test]
fn rejects_invalid_input() {
    let mut decoder = Decoder::default();
    assert!(decoder.decode(&[0x80]).is_err()); // index 0
    assert!(decoder.decode(&[0xff, 0xff, 0xff, 0xff, 0xff]).is_err()); // index too large
    assert!(decoder.decode(&[0x40, 0x01, b'A']).is_err()); // uppercase name
    assert!(decoder.decode(&[0x00, 0x05]).is_err()); // truncated name
}
