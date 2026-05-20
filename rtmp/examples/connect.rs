//! End-to-end connect probe.
//!
//! Usage: `cargo run --release -p ruffle_rtmp --example connect rtmpe://127.0.0.1:1935/master/test`
//!
//! Drives `ruffle_rtmp::connect()` and prints every inbound event for ~5
//! seconds. Useful to confirm the handshake reaches a real server without
//! needing a SWF in the loop.

use std::time::{Duration, Instant};

fn main() {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "rtmp://127.0.0.1:1935/master/test".into());

    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "ruffle_rtmp=debug".into());
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
        .with_writer(std::io::stderr)
        .init();

    // Optional second arg: hex-encoded extra-args AMF blob (whitespace ignored)
    // so we can replay the bytes a real SWF sends past the URL.
    let extra_args: Vec<u8> = std::env::args()
        .nth(2)
        .map(|hex| {
            let hex: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
            (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect()
        })
        .unwrap_or_default();

    // Optional third / fourth args: swfUrl and pageUrl. Default to a plausible
    // dummy so we don't ship an empty swfUrl by default (some servers reject).
    let swf_url = std::env::args()
        .nth(3)
        .unwrap_or_else(|| "file:///probe.swf".into());
    let page_url = std::env::args().nth(4).unwrap_or_default();

    eprintln!(
        "[probe] connecting to {url} swf_url={swf_url:?} page_url={page_url:?} \
         (extra_args={} bytes)",
        extra_args.len()
    );
    let handle = ruffle_rtmp::connect(&url, &swf_url, &page_url, &extra_args);
    if handle == 0 {
        eprintln!("[probe] connect rejected (bad URL or unsupported scheme)");
        std::process::exit(1);
    }
    eprintln!("[probe] handle = {handle}; draining events for 5s …");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut fired_call = false;
    while Instant::now() < deadline {
        let events = ruffle_rtmp::drain();
        for ev in &events {
            eprintln!("[probe] {ev:?}");
        }
        // Right after Connect.Success, fire a no-op NetConnection.call() to
        // exercise the post-connect write path (this is the deadlock case).
        if !fired_call
            && events.iter().any(|e| matches!(
                e,
                ruffle_rtmp::InboundEvent::Status { code, .. } if code == "NetConnection.Connect.Success"
            ))
        {
            fired_call = true;
            // AMF0: command name "ping", txid=2, null command-object, no args.
            let mut payload = Vec::new();
            payload.push(0x02); // String
            payload.extend_from_slice(&(4u16.to_be_bytes()));
            payload.extend_from_slice(b"ping");
            payload.push(0x00); // Number marker
            payload.extend_from_slice(&2.0f64.to_be_bytes());
            payload.push(0x05); // Null
            eprintln!("[probe] firing test call('ping', txid=2)");
            ruffle_rtmp::call(handle, 2, &payload);
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    ruffle_rtmp::close(handle);
    // Give the worker a moment to flush a Closed status.
    std::thread::sleep(Duration::from_millis(100));
    for ev in ruffle_rtmp::drain() {
        eprintln!("[probe] {ev:?}");
    }
}
