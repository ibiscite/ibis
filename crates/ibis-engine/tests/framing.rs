use ibis_engine::protocol::{FrameError, MAX_FRAME_BYTES, read_frame, write_frame};
use serde_json::json;
use std::io::{BufReader, Cursor};

#[test]
fn unicode_and_multiple_frames_use_exact_byte_lengths() -> Result<(), Box<dyn std::error::Error>> {
    let messages = [
        json!({"jsonrpc":"2.0","id":1,"method":"show","params":{"text":"文献 العربية 🦉"}}),
        json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":1}}),
    ];
    let mut bytes = vec![];
    for message in &messages {
        write_frame(&mut bytes, message)?;
    }
    let mut reader = BufReader::new(Cursor::new(bytes));
    for message in messages {
        assert_eq!(read_frame(&mut reader)?, Some(message));
    }
    assert_eq!(read_frame(&mut reader)?, None);
    Ok(())
}

#[test]
fn malformed_headers_and_truncated_messages_fail_safely() {
    for bytes in [
        b"Content-Length: 2\r\nContent-Length: 2\r\n\r\n{}".to_vec(),
        b"Content-Length: +2\r\n\r\n{}".to_vec(),
        b"Content-Length: 2\n\n{}".to_vec(),
        b"Content-Length: 3\r\n\r\n{}".to_vec(),
        b"Content-Length: 0\r\n\r\n".to_vec(),
        b"Other: yes\r\n\r\n{}".to_vec(),
        b"Content-Length: 2\r\nOther: \x7f\r\n\r\n{}".to_vec(),
        b"Content-Length: 2\r\nContent-Type: application/json; charset=utf-16\r\n\r\n{}".to_vec(),
    ] {
        assert!(read_frame(&mut Cursor::new(bytes)).is_err());
    }
}

#[test]
fn hostile_sizes_are_rejected_before_reading_body() {
    let header = format!("Content-Length: {}\r\n\r\n", MAX_FRAME_BYTES + 1);
    let mut reader = Cursor::new(format!("{header}unread"));
    assert_eq!(read_frame(&mut reader), Err(FrameError::TooLarge));
    assert!(reader.position() <= header.len() as u64);
    assert_eq!(
        read_frame(&mut Cursor::new(vec![b'x'; 9_000])),
        Err(FrameError::TooLarge)
    );
    let mut output = vec![];
    assert_eq!(
        write_frame(&mut output, &json!("x".repeat(MAX_FRAME_BYTES))),
        Err(FrameError::TooLarge)
    );
    assert!(output.is_empty());
}

#[test]
fn duplicate_json_members_are_refused_including_nested_parameters() {
    for body in [
        r#"{"method":"search","method":"cite"}"#,
        r#"{"params":{"query":"a","query":"b"}}"#,
        r#"{"method":"search","\u006dethod":"cite"}"#,
    ] {
        let frame = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        assert_eq!(read_frame(&mut Cursor::new(frame)), Err(FrameError::InvalidJson));
    }
}
