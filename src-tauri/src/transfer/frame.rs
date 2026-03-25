use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Maximum allowed frame payload size (64 MiB).
pub const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;

// ---- Pure encode/decode (sync, testable) ----

/// Encode a payload as a length-prefixed frame: [4-byte BE len][payload].
pub fn encode(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

/// Try to decode a frame from a byte slice.
/// Returns `Some((payload, remainder))` if a complete frame is present.
#[allow(dead_code)]
pub fn decode(buf: &[u8]) -> Option<(&[u8], &[u8])> {
    if buf.len() < 4 {
        return None;
    }
    let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if buf.len() < 4 + len {
        return None;
    }
    Some((&buf[4..4 + len], &buf[4 + len..]))
}

// ---- Async I/O helpers ----

/// Read a single length-prefixed frame from `reader`.
pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> anyhow::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    anyhow::ensure!(
        len <= MAX_FRAME_BYTES,
        "frame too large: {len} bytes (max {MAX_FRAME_BYTES})"
    );
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await?;
    Ok(payload)
}

/// Write a single length-prefixed frame to `writer`.
pub async fn write_frame<W: AsyncWrite + Unpin>(writer: &mut W, payload: &[u8]) -> anyhow::Result<()> {
    let frame = encode(payload);
    writer.write_all(&frame).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[test]
    fn encode_has_correct_length_prefix() {
        let payload = b"hello";
        let frame = encode(payload);
        assert_eq!(&frame[..4], &5u32.to_be_bytes());
        assert_eq!(&frame[4..], b"hello");
    }

    #[test]
    fn encode_empty_payload() {
        let frame = encode(b"");
        assert_eq!(frame.len(), 4);
        assert_eq!(&frame[..4], &0u32.to_be_bytes());
    }

    #[test]
    fn decode_complete_frame() {
        let payload = b"panote-frame";
        let frame = encode(payload);
        let (decoded, remainder) = decode(&frame).unwrap();
        assert_eq!(decoded, payload);
        assert!(remainder.is_empty());
    }

    #[test]
    fn decode_with_trailing_data() {
        let payload = b"first";
        let mut frame = encode(payload);
        frame.extend_from_slice(b"garbage");
        let (decoded, remainder) = decode(&frame).unwrap();
        assert_eq!(decoded, b"first");
        assert_eq!(remainder, b"garbage");
    }

    #[test]
    fn decode_incomplete_header_returns_none() {
        assert!(decode(&[0, 0]).is_none());
    }

    #[test]
    fn decode_incomplete_payload_returns_none() {
        let mut frame = encode(b"hello world");
        frame.truncate(7); // cut the payload short
        assert!(decode(&frame).is_none());
    }

    #[test]
    fn decode_multiple_frames_sequentially() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&encode(b"one"));
        buf.extend_from_slice(&encode(b"two"));
        buf.extend_from_slice(&encode(b"three"));

        let (a, rest) = decode(&buf).unwrap();
        assert_eq!(a, b"one");
        let (b, rest) = decode(rest).unwrap();
        assert_eq!(b, b"two");
        let (c, rest) = decode(rest).unwrap();
        assert_eq!(c, b"three");
        assert!(rest.is_empty());
    }

    #[tokio::test]
    async fn async_write_read_roundtrip() {
        let (mut client, mut server) = duplex(1024);
        let payload = b"async frame test";

        write_frame(&mut client, payload).await.unwrap();
        let received = read_frame(&mut server).await.unwrap();
        assert_eq!(received, payload);
    }

    #[tokio::test]
    async fn async_multiple_frames() {
        let (mut client, mut server) = duplex(4096);

        for i in 0u8..10 {
            write_frame(&mut client, &[i; 100]).await.unwrap();
        }
        drop(client);

        for i in 0u8..10 {
            let frame = read_frame(&mut server).await.unwrap();
            assert_eq!(frame, vec![i; 100]);
        }
    }

    #[tokio::test]
    async fn read_frame_rejects_oversized() {
        let (mut client, mut server) = duplex(16);
        // Write a header claiming MAX_FRAME_BYTES + 1
        let giant_len = (MAX_FRAME_BYTES + 1) as u32;
        client.write_all(&giant_len.to_be_bytes()).await.unwrap();

        let result = read_frame(&mut server).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }
}
