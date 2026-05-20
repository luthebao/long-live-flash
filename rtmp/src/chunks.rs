//! RTMP chunk-stream reader/writer + transparent RC4 layer for RTMPE.
//!
//! The connection is split into a [`ReadHalf`] and [`WriteHalf`] backed by
//! two `TcpStream::try_clone` handles to the same kernel socket. Each half
//! owns the per-direction state (chunk size, RC4 keystream, accumulator
//! buffers). This avoids holding any lock across a blocking socket read.
//!
//! Control-plane messages (SetChunkSize, Ack, Window Ack Size, Set Peer
//! Bandwidth, User Control) are absorbed inline by the reader — application
//! callers only see AMF command/data, audio, and video messages.
//!
//! Mirrors the chunk-stream code in `src/rtmp.odin`.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

use crate::RtmpError;
use crate::handshake::Rc4;

pub const DEFAULT_CHUNK: usize = 128;
pub const MAX_CHUNK: usize = 65536;
pub const WINDOW_ACK_SIZE: u32 = 2_500_000;

// Message type IDs the chunk-stream reader/writer actually checks. The
// audio/video/data message-type ids exist on the wire but we don't dispatch
// them at this layer — the AVM-side NetStream code would consume those once
// implemented.
pub const MSG_SET_CHUNK_SIZE: u8 = 1;
pub const MSG_ABORT: u8 = 2;
pub const MSG_ACK: u8 = 3;
pub const MSG_USER_CONTROL: u8 = 4;
pub const MSG_WIN_ACK_SIZE: u8 = 5;
pub const MSG_SET_PEER_BW: u8 = 6;
pub const MSG_AMF3_COMMAND: u8 = 17;
pub const MSG_AMF0_COMMAND: u8 = 20;

const CHUNK_STREAMS: usize = 64;

#[derive(Default)]
struct ChunkStream {
    msg_type: u8,
    msg_stream: u32,
    msg_length: u32,
    timestamp: u32,
    extended_ts: bool,
    payload: Vec<u8>,
}

pub struct Message {
    pub msg_type: u8,
    pub payload: Vec<u8>,
}

// --- Write side ----------------------------------------------------------

pub struct WriteHalf {
    sock: TcpStream,
    pub out_chunk_size: usize,
    rc4_out: Option<Rc4>,
}

impl WriteHalf {
    pub fn shutdown(&self) {
        let _ = self.sock.shutdown(std::net::Shutdown::Both);
    }

    fn send_all(&mut self, buf: &[u8]) -> Result<(), RtmpError> {
        if tracing::enabled!(tracing::Level::TRACE) {
            tracing::trace!("tx {} bytes: {}", buf.len(), hex_preview(buf));
        }
        if let Some(rc4) = &mut self.rc4_out {
            // RC4 mutates state; encrypt a temp copy so callers' buffers
            // (which may be reused) are left alone.
            let mut tmp = buf.to_vec();
            rc4.crypt(&mut tmp);
            self.sock.write_all(&tmp)?;
        } else {
            self.sock.write_all(buf)?;
        }
        Ok(())
    }

    /// Write one RTMP message split into chunks no larger than
    /// `out_chunk_size`. Always uses type-0 chunks for the first piece
    /// (full header) and type-3 for continuations.
    pub fn send_message(
        &mut self,
        csid: u32,
        msg_type: u8,
        msg_stream: u32,
        payload: &[u8],
    ) -> Result<(), RtmpError> {
        let mut buf: Vec<u8> = Vec::with_capacity(12 + payload.len());

        // Type-0 header: basic(1) + timestamp(3) + msg_length(3) + type(1) + stream(4 LE)
        buf.push(csid as u8 & 0x3f);
        buf.extend_from_slice(&[0, 0, 0]);
        let len = payload.len() as u32;
        buf.extend_from_slice(&[(len >> 16) as u8, (len >> 8) as u8, len as u8]);
        buf.push(msg_type);
        buf.extend_from_slice(&msg_stream.to_le_bytes());

        let mut sent = 0;
        let mut first = true;
        while sent < payload.len() {
            if !first {
                buf.push((3 << 6) | (csid as u8 & 0x3f));
            }
            first = false;
            let chunk_end = (sent + self.out_chunk_size).min(payload.len());
            buf.extend_from_slice(&payload[sent..chunk_end]);
            sent = chunk_end;
        }
        self.send_all(&buf)
    }

    pub fn send_window_ack_size(&mut self, win: u32) -> Result<(), RtmpError> {
        self.send_message(2, MSG_WIN_ACK_SIZE, 0, &win.to_be_bytes())
    }

    pub fn send_set_chunk_size(&mut self, size: u32) -> Result<(), RtmpError> {
        self.send_message(2, MSG_SET_CHUNK_SIZE, 0, &size.to_be_bytes())?;
        self.out_chunk_size = size as usize;
        Ok(())
    }
}

pub type SharedWriter = Arc<Mutex<WriteHalf>>;

// --- Read side -----------------------------------------------------------

pub struct ReadHalf {
    sock: TcpStream,
    pub in_chunk_size: usize,
    recv_streams: Vec<ChunkStream>,
    bytes_in: u32,
    peer_window: u32,
    rc4_in: Option<Rc4>,
    /// Shared writer; used only to send infrequent acks and User-Control
    /// ping replies back over the same socket.
    writer: Option<SharedWriter>,
}

impl ReadHalf {
    pub fn attach_writer(&mut self, writer: SharedWriter) {
        self.writer = Some(writer);
    }

    fn recv_exact(&mut self, buf: &mut [u8]) -> Result<(), RtmpError> {
        let mut got = 0;
        while got < buf.len() {
            let n = self.sock.read(&mut buf[got..])?;
            if n == 0 {
                return Err(RtmpError::Eof);
            }
            if let Some(rc4) = &mut self.rc4_in {
                rc4.crypt(&mut buf[got..got + n]);
            }
            if tracing::enabled!(tracing::Level::TRACE) {
                tracing::trace!("rx {} bytes: {}", n, hex_preview(&buf[got..got + n]));
            }
            got += n;
            self.bytes_in = self.bytes_in.wrapping_add(n as u32);
        }
        // Ack handling: send ack whenever we've crossed the peer's window.
        if self.peer_window != 0 && self.bytes_in >= self.peer_window {
            let total = self.bytes_in;
            self.bytes_in = 0;
            self.send_via_writer(2, MSG_ACK, 0, &total.to_be_bytes())?;
        }
        Ok(())
    }

    fn send_via_writer(
        &self,
        csid: u32,
        msg_type: u8,
        msg_stream: u32,
        payload: &[u8],
    ) -> Result<(), RtmpError> {
        if let Some(w) = &self.writer {
            w.lock().unwrap().send_message(csid, msg_type, msg_stream, payload)?;
        }
        Ok(())
    }

    /// Block until one full application-layer message has been reassembled.
    /// Control messages are absorbed inline.
    pub fn read_message(&mut self) -> Result<Message, RtmpError> {
        loop {
            let mut bh = [0u8; 1];
            self.recv_exact(&mut bh)?;
            let fmt_id = bh[0] >> 6;
            let csid_lo = (bh[0] & 0x3f) as u32;
            tracing::debug!("chunk: fmt={fmt_id} csid_lo={csid_lo} (basic=0x{:02x})", bh[0]);

            let csid = match csid_lo {
                0 => {
                    let mut b = [0u8; 1];
                    self.recv_exact(&mut b)?;
                    b[0] as u32 + 64
                }
                1 => {
                    let mut b = [0u8; 2];
                    self.recv_exact(&mut b)?;
                    b[0] as u32 + b[1] as u32 * 256 + 64
                }
                _ => csid_lo,
            };
            let cs_idx = (csid as usize).min(CHUNK_STREAMS - 1);

            let mut maybe_reset = None;
            match fmt_id {
                0 => {
                    let mut mh = [0u8; 11];
                    self.recv_exact(&mut mh)?;
                    let mut ts = u32::from(mh[0]) << 16 | u32::from(mh[1]) << 8 | u32::from(mh[2]);
                    let len_ = u32::from(mh[3]) << 16 | u32::from(mh[4]) << 8 | u32::from(mh[5]);
                    let type_ = mh[6];
                    let stream =
                        u32::from(mh[7]) | u32::from(mh[8]) << 8 | u32::from(mh[9]) << 16 | u32::from(mh[10]) << 24;
                    let mut ext = false;
                    if ts == 0x00ff_ffff {
                        let mut e = [0u8; 4];
                        self.recv_exact(&mut e)?;
                        ts = u32::from_be_bytes(e);
                        ext = true;
                    }
                    maybe_reset = Some((ts, len_, type_, stream, ext));
                }
                1 => {
                    let mut mh = [0u8; 7];
                    self.recv_exact(&mut mh)?;
                    let mut td = u32::from(mh[0]) << 16 | u32::from(mh[1]) << 8 | u32::from(mh[2]);
                    let len_ = u32::from(mh[3]) << 16 | u32::from(mh[4]) << 8 | u32::from(mh[5]);
                    let type_ = mh[6];
                    let mut ext = false;
                    if td == 0x00ff_ffff {
                        let mut e = [0u8; 4];
                        self.recv_exact(&mut e)?;
                        td = u32::from_be_bytes(e);
                        ext = true;
                    }
                    let rs = &mut self.recv_streams[cs_idx];
                    rs.timestamp = rs.timestamp.wrapping_add(td);
                    rs.msg_length = len_;
                    rs.msg_type = type_;
                    rs.extended_ts = ext;
                    rs.payload.clear();
                }
                2 => {
                    let mut mh = [0u8; 3];
                    self.recv_exact(&mut mh)?;
                    let mut td = u32::from(mh[0]) << 16 | u32::from(mh[1]) << 8 | u32::from(mh[2]);
                    let mut ext = false;
                    if td == 0x00ff_ffff {
                        let mut e = [0u8; 4];
                        self.recv_exact(&mut e)?;
                        td = u32::from_be_bytes(e);
                        ext = true;
                    }
                    let rs = &mut self.recv_streams[cs_idx];
                    rs.timestamp = rs.timestamp.wrapping_add(td);
                    rs.extended_ts = ext;
                    rs.payload.clear();
                }
                _ => {
                    // fmt 3: continuation. If the originating chunk had an
                    // extended timestamp, an extended timestamp also rides
                    // each type-3 continuation; read and discard it.
                    if self.recv_streams[cs_idx].extended_ts {
                        let mut e = [0u8; 4];
                        self.recv_exact(&mut e)?;
                    }
                }
            }
            if let Some((ts, len_, type_, stream, ext)) = maybe_reset {
                let rs = &mut self.recv_streams[cs_idx];
                rs.timestamp = ts;
                rs.msg_length = len_;
                rs.msg_type = type_;
                rs.msg_stream = stream;
                rs.extended_ts = ext;
                rs.payload.clear();
            }

            let in_chunk_size = self.in_chunk_size;
            let target_len = self.recv_streams[cs_idx].msg_length as usize;
            let remaining = target_len.saturating_sub(self.recv_streams[cs_idx].payload.len());
            let to_read = remaining.min(in_chunk_size);
            if to_read > 0 {
                let mut tmp = vec![0u8; to_read];
                self.recv_exact(&mut tmp)?;
                self.recv_streams[cs_idx].payload.extend_from_slice(&tmp);
            }

            if self.recv_streams[cs_idx].payload.len() < target_len {
                continue;
            }

            let payload = std::mem::take(&mut self.recv_streams[cs_idx].payload);
            let msg = Message {
                msg_type: self.recv_streams[cs_idx].msg_type,
                payload,
            };
            tracing::debug!(
                "assembled msg: type={} ({}) len={} csid={}",
                msg.msg_type,
                msg_type_name(msg.msg_type),
                msg.payload.len(),
                csid,
            );

            if self.handle_control(&msg)? {
                continue;
            }
            return Ok(msg);
        }
    }

    fn handle_control(&mut self, msg: &Message) -> Result<bool, RtmpError> {
        match msg.msg_type {
            MSG_SET_CHUNK_SIZE => {
                if msg.payload.len() >= 4 {
                    let v = (u32::from_be_bytes([
                        msg.payload[0],
                        msg.payload[1],
                        msg.payload[2],
                        msg.payload[3],
                    ]) & 0x7fff_ffff) as usize;
                    if v > 0 && v <= MAX_CHUNK {
                        self.in_chunk_size = v;
                    }
                }
                Ok(true)
            }
            MSG_ACK | MSG_ABORT => Ok(true),
            MSG_WIN_ACK_SIZE => {
                if msg.payload.len() >= 4 {
                    self.peer_window = u32::from_be_bytes([
                        msg.payload[0],
                        msg.payload[1],
                        msg.payload[2],
                        msg.payload[3],
                    ]);
                }
                Ok(true)
            }
            MSG_SET_PEER_BW => {
                if msg.payload.len() >= 4 {
                    let win = u32::from_be_bytes([
                        msg.payload[0],
                        msg.payload[1],
                        msg.payload[2],
                        msg.payload[3],
                    ]);
                    self.send_via_writer(2, MSG_WIN_ACK_SIZE, 0, &win.to_be_bytes())?;
                }
                Ok(true)
            }
            MSG_USER_CONTROL => {
                // Stream Begin, Ping Request, etc. Only Ping Request (evt=6)
                // needs a response (evt=7 with the same 4-byte timestamp).
                if msg.payload.len() >= 6 {
                    let evt = u16::from_be_bytes([msg.payload[0], msg.payload[1]]);
                    if evt == 6 {
                        let mut resp = [0u8; 6];
                        resp[0] = 0;
                        resp[1] = 7;
                        resp[2..].copy_from_slice(&msg.payload[2..6]);
                        self.send_via_writer(2, MSG_USER_CONTROL, 0, &resp)?;
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

// --- Combined connection (used during dial + handshake + connect) --------
//
// Before the handshake completes and the worker hands the connection off to
// the per-connection reader thread, all I/O happens single-threaded; we use
// `Conn` for that phase. After connect succeeds, `into_halves` splits the
// socket into independent read/write halves so the reader thread can block
// in `read_message` without holding a lock the writer would need.

pub struct Conn {
    read: ReadHalf,
    write: WriteHalf,
}

impl Conn {
    pub fn new(sock: TcpStream) -> Result<Self, RtmpError> {
        let read_sock = sock.try_clone()?;
        let mut recv_streams = Vec::with_capacity(CHUNK_STREAMS);
        for _ in 0..CHUNK_STREAMS {
            recv_streams.push(ChunkStream::default());
        }
        Ok(Self {
            read: ReadHalf {
                sock: read_sock,
                in_chunk_size: DEFAULT_CHUNK,
                recv_streams,
                bytes_in: 0,
                peer_window: WINDOW_ACK_SIZE,
                rc4_in: None,
                writer: None,
            },
            write: WriteHalf {
                sock,
                out_chunk_size: DEFAULT_CHUNK,
                rc4_out: None,
            },
        })
    }

    pub fn enable_rc4(&mut self, rc4_in: Rc4, rc4_out: Rc4) {
        self.read.rc4_in = Some(rc4_in);
        self.write.rc4_out = Some(rc4_out);
    }

    pub fn read_message(&mut self) -> Result<Message, RtmpError> {
        // We can't reuse `ReadHalf::read_message` here because it needs an
        // `Option<SharedWriter>` to bounce control replies back, and a Conn
        // doesn't have one yet (we'd have to wrap our own WriteHalf in an
        // Arc<Mutex<>>, then lock that same writer to satisfy borrowing —
        // which is fine but slower). During the single-threaded handshake +
        // connect phase we inline the read loop here and call self.write
        // directly for any control responses.
        loop {
            let mut bh = [0u8; 1];
            read_exact_with_rc4(&mut self.read, &mut bh)?;
            let fmt_id = bh[0] >> 6;
            let csid_lo = (bh[0] & 0x3f) as u32;
            tracing::debug!("chunk: fmt={fmt_id} csid_lo={csid_lo} (basic=0x{:02x})", bh[0]);

            let csid = match csid_lo {
                0 => {
                    let mut b = [0u8; 1];
                    read_exact_with_rc4(&mut self.read, &mut b)?;
                    b[0] as u32 + 64
                }
                1 => {
                    let mut b = [0u8; 2];
                    read_exact_with_rc4(&mut self.read, &mut b)?;
                    b[0] as u32 + b[1] as u32 * 256 + 64
                }
                _ => csid_lo,
            };
            let cs_idx = (csid as usize).min(CHUNK_STREAMS - 1);

            let mut maybe_reset = None;
            match fmt_id {
                0 => {
                    let mut mh = [0u8; 11];
                    read_exact_with_rc4(&mut self.read, &mut mh)?;
                    let mut ts = u32::from(mh[0]) << 16 | u32::from(mh[1]) << 8 | u32::from(mh[2]);
                    let len_ = u32::from(mh[3]) << 16 | u32::from(mh[4]) << 8 | u32::from(mh[5]);
                    let type_ = mh[6];
                    let stream =
                        u32::from(mh[7]) | u32::from(mh[8]) << 8 | u32::from(mh[9]) << 16 | u32::from(mh[10]) << 24;
                    let mut ext = false;
                    if ts == 0x00ff_ffff {
                        let mut e = [0u8; 4];
                        read_exact_with_rc4(&mut self.read, &mut e)?;
                        ts = u32::from_be_bytes(e);
                        ext = true;
                    }
                    maybe_reset = Some((ts, len_, type_, stream, ext));
                }
                1 => {
                    let mut mh = [0u8; 7];
                    read_exact_with_rc4(&mut self.read, &mut mh)?;
                    let mut td = u32::from(mh[0]) << 16 | u32::from(mh[1]) << 8 | u32::from(mh[2]);
                    let len_ = u32::from(mh[3]) << 16 | u32::from(mh[4]) << 8 | u32::from(mh[5]);
                    let type_ = mh[6];
                    let mut ext = false;
                    if td == 0x00ff_ffff {
                        let mut e = [0u8; 4];
                        read_exact_with_rc4(&mut self.read, &mut e)?;
                        td = u32::from_be_bytes(e);
                        ext = true;
                    }
                    let rs = &mut self.read.recv_streams[cs_idx];
                    rs.timestamp = rs.timestamp.wrapping_add(td);
                    rs.msg_length = len_;
                    rs.msg_type = type_;
                    rs.extended_ts = ext;
                    rs.payload.clear();
                }
                2 => {
                    let mut mh = [0u8; 3];
                    read_exact_with_rc4(&mut self.read, &mut mh)?;
                    let mut td = u32::from(mh[0]) << 16 | u32::from(mh[1]) << 8 | u32::from(mh[2]);
                    let mut ext = false;
                    if td == 0x00ff_ffff {
                        let mut e = [0u8; 4];
                        read_exact_with_rc4(&mut self.read, &mut e)?;
                        td = u32::from_be_bytes(e);
                        ext = true;
                    }
                    let rs = &mut self.read.recv_streams[cs_idx];
                    rs.timestamp = rs.timestamp.wrapping_add(td);
                    rs.extended_ts = ext;
                    rs.payload.clear();
                }
                _ => {
                    if self.read.recv_streams[cs_idx].extended_ts {
                        let mut e = [0u8; 4];
                        read_exact_with_rc4(&mut self.read, &mut e)?;
                    }
                }
            }
            if let Some((ts, len_, type_, stream, ext)) = maybe_reset {
                let rs = &mut self.read.recv_streams[cs_idx];
                rs.timestamp = ts;
                rs.msg_length = len_;
                rs.msg_type = type_;
                rs.msg_stream = stream;
                rs.extended_ts = ext;
                rs.payload.clear();
            }

            let in_chunk_size = self.read.in_chunk_size;
            let target_len = self.read.recv_streams[cs_idx].msg_length as usize;
            let remaining = target_len.saturating_sub(self.read.recv_streams[cs_idx].payload.len());
            let to_read = remaining.min(in_chunk_size);
            if to_read > 0 {
                let mut tmp = vec![0u8; to_read];
                read_exact_with_rc4(&mut self.read, &mut tmp)?;
                self.read.recv_streams[cs_idx].payload.extend_from_slice(&tmp);
            }
            if self.read.recv_streams[cs_idx].payload.len() < target_len {
                continue;
            }

            let payload = std::mem::take(&mut self.read.recv_streams[cs_idx].payload);
            let msg = Message {
                msg_type: self.read.recv_streams[cs_idx].msg_type,
                payload,
            };
            tracing::debug!(
                "assembled msg: type={} ({}) len={} csid={}",
                msg.msg_type,
                msg_type_name(msg.msg_type),
                msg.payload.len(),
                csid,
            );

            // Inline control handling (single-threaded phase): the reader
            // and writer are in the same thread; we can call self.write
            // directly.
            match msg.msg_type {
                MSG_SET_CHUNK_SIZE => {
                    if msg.payload.len() >= 4 {
                        let v = (u32::from_be_bytes([
                            msg.payload[0],
                            msg.payload[1],
                            msg.payload[2],
                            msg.payload[3],
                        ]) & 0x7fff_ffff) as usize;
                        if v > 0 && v <= MAX_CHUNK {
                            self.read.in_chunk_size = v;
                        }
                    }
                    continue;
                }
                MSG_ACK | MSG_ABORT => continue,
                MSG_WIN_ACK_SIZE => {
                    if msg.payload.len() >= 4 {
                        self.read.peer_window = u32::from_be_bytes([
                            msg.payload[0],
                            msg.payload[1],
                            msg.payload[2],
                            msg.payload[3],
                        ]);
                    }
                    continue;
                }
                MSG_SET_PEER_BW => {
                    if msg.payload.len() >= 4 {
                        let win = u32::from_be_bytes([
                            msg.payload[0],
                            msg.payload[1],
                            msg.payload[2],
                            msg.payload[3],
                        ]);
                        self.write.send_message(2, MSG_WIN_ACK_SIZE, 0, &win.to_be_bytes())?;
                    }
                    continue;
                }
                MSG_USER_CONTROL => {
                    if msg.payload.len() >= 6 {
                        let evt = u16::from_be_bytes([msg.payload[0], msg.payload[1]]);
                        if evt == 6 {
                            let mut resp = [0u8; 6];
                            resp[0] = 0;
                            resp[1] = 7;
                            resp[2..].copy_from_slice(&msg.payload[2..6]);
                            self.write.send_message(2, MSG_USER_CONTROL, 0, &resp)?;
                        }
                    }
                    continue;
                }
                _ => return Ok(msg),
            }
        }
    }

    pub fn send_window_ack_size(&mut self, win: u32) -> Result<(), RtmpError> {
        self.write.send_window_ack_size(win)
    }
    pub fn send_set_chunk_size(&mut self, size: u32) -> Result<(), RtmpError> {
        self.write.send_set_chunk_size(size)
    }
    pub fn send_message(
        &mut self,
        csid: u32,
        msg_type: u8,
        msg_stream: u32,
        payload: &[u8],
    ) -> Result<(), RtmpError> {
        self.write.send_message(csid, msg_type, msg_stream, payload)
    }

    /// Decompose into the two halves used in steady state. The reader half
    /// gets a clone of the shared writer so it can bounce acks / ping
    /// responses back over the same socket without locking anything else.
    pub fn into_halves(self) -> (ReadHalf, SharedWriter) {
        let shared = Arc::new(Mutex::new(self.write));
        let mut read = self.read;
        read.attach_writer(shared.clone());
        (read, shared)
    }
}

fn read_exact_with_rc4(read: &mut ReadHalf, buf: &mut [u8]) -> Result<(), RtmpError> {
    let mut got = 0;
    while got < buf.len() {
        let n = read.sock.read(&mut buf[got..])?;
        if n == 0 {
            return Err(RtmpError::Eof);
        }
        if let Some(rc4) = &mut read.rc4_in {
            rc4.crypt(&mut buf[got..got + n]);
        }
        got += n;
        read.bytes_in = read.bytes_in.wrapping_add(n as u32);
    }
    Ok(())
}

fn hex_preview(bytes: &[u8]) -> String {
    let n = bytes.len().min(64);
    let mut s = String::with_capacity(n * 3 + 16);
    for b in &bytes[..n] {
        s.push_str(&format!("{b:02x} "));
    }
    if bytes.len() > n {
        s.push_str(&format!("... (+{})", bytes.len() - n));
    }
    s
}

fn msg_type_name(t: u8) -> &'static str {
    match t {
        1 => "SetChunkSize",
        2 => "Abort",
        3 => "Ack",
        4 => "UserControl",
        5 => "WindowAckSize",
        6 => "SetPeerBandwidth",
        8 => "Audio",
        9 => "Video",
        15 => "AMF3Data",
        17 => "AMF3Command",
        18 => "AMF0Data",
        20 => "AMF0Command",
        _ => "Unknown",
    }
}
