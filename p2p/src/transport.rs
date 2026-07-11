//! Unifies raw TCP and WebSocket as interchangeable P2P transports, so
//! `handle_peer_connection` (see server.rs) doesn't need a second copy of
//! its message-handling logic per transport. WebSocket exists purely so a
//! node that can't be dialed via raw TCP (e.g. hosted behind a platform like
//! Render that only proxies a single HTTP(S) port - see the module doc on
//! why this exists) can still be joined by peers dialing out over
//! wss://<host>/v1/p2p/ws.
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use futures_util::{SinkExt, StreamExt};
use futures_util::stream::{SplitSink, SplitStream};
use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

use super::message::P2pMessage;

/// Maximum allowed size (in bytes) for a single P2P message, on either
/// transport. For TCP this guards the length-prefix header against a peer
/// claiming an oversized length; for WebSocket it's passed as the
/// max_message_size/max_frame_size config on both the warp server side and
/// the tokio-tungstenite client side.
pub const MAX_MESSAGE_SIZE: usize = 32 * 1024 * 1024;

pub type WsClientStream = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>;

pub enum PeerReader {
    Tcp(OwnedReadHalf),
    /// Inbound WebSocket connection, accepted via warp::ws() in api/server.rs.
    WsServer(SplitStream<warp::ws::WebSocket>),
    /// Outbound WebSocket connection, dialed via tokio_tungstenite::connect_async_with_config.
    WsClient(SplitStream<WsClientStream>),
}

pub enum PeerWriter {
    Tcp(OwnedWriteHalf),
    WsServer(SplitSink<warp::ws::WebSocket, warp::ws::Message>),
    WsClient(SplitSink<WsClientStream, TungsteniteMessage>),
}

/// Reads the next P2pMessage off this peer connection, regardless of
/// transport. TCP is a raw byte stream with no message boundaries of its
/// own, so it keeps the 4-byte length-prefix framing; a WebSocket frame
/// already has its own boundary, so one binary frame is exactly one
/// P2pMessage. Non-data frames (ping/pong/text) are skipped internally
/// rather than surfaced - the existing app-level P2pMessage::Ping/Pong
/// already provides liveness, so WS control frames don't need to reach the
/// caller. Returns None on a clean disconnect (or an oversized/garbled
/// message, which is treated the same as a disconnect).
pub async fn read_message(reader: &mut PeerReader) -> Option<P2pMessage> {
    match reader {
        PeerReader::Tcp(read_half) => {
            let mut len_bytes = [0u8; 4];
            read_half.read_exact(&mut len_bytes).await.ok()?;
            let len = u32::from_le_bytes(len_bytes) as usize;
            if len > MAX_MESSAGE_SIZE {
                println!("P2P: Peer sent oversized message ({} bytes), disconnecting", len);
                return None;
            }
            let mut buf = vec![0u8; len];
            read_half.read_exact(&mut buf).await.ok()?;
            bincode::deserialize::<P2pMessage>(&buf).ok()
        }
        PeerReader::WsServer(stream) => {
            loop {
                match stream.next().await {
                    Some(Ok(msg)) if msg.is_binary() => {
                        return bincode::deserialize::<P2pMessage>(msg.as_bytes()).ok();
                    }
                    Some(Ok(msg)) if msg.is_close() => return None,
                    Some(Ok(_)) => continue,
                    Some(Err(_)) | None => return None,
                }
            }
        }
        PeerReader::WsClient(stream) => {
            loop {
                match stream.next().await {
                    Some(Ok(TungsteniteMessage::Binary(bytes))) => {
                        return bincode::deserialize::<P2pMessage>(&bytes).ok();
                    }
                    Some(Ok(TungsteniteMessage::Close(_))) | None => return None,
                    Some(Ok(_)) => continue,
                    Some(Err(_)) => return None,
                }
            }
        }
    }
}

/// Writes a single P2pMessage to this peer connection, regardless of
/// transport - the counterpart to read_message.
pub async fn write_message(writer: &mut PeerWriter, msg: &P2pMessage) -> std::io::Result<()> {
    let bytes = bincode::serialize(msg).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    match writer {
        PeerWriter::Tcp(write_half) => {
            let len = bytes.len() as u32;
            write_half.write_all(&len.to_le_bytes()).await?;
            write_half.write_all(&bytes).await?;
            write_half.flush().await?;
            Ok(())
        }
        PeerWriter::WsServer(sink) => {
            sink.send(warp::ws::Message::binary(bytes)).await
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
        }
        PeerWriter::WsClient(sink) => {
            sink.send(TungsteniteMessage::Binary(bytes)).await
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
        }
    }
}
