/**
 * Main-world RTMP bridge for the Llflash browser extension.
 *
 * Lives next to the wasm player in MAIN world. Outbound: wasm calls the
 * `bridgeOut` closure on every AVM `NetConnection` op against an RTMP
 * URL; we relay via `window.postMessage` to our own content script.
 * Inbound: content script forwards native messaging events back as
 * `window.postMessage`; we dispatch them onto every registered player's
 * `dispatchRtmp*` method. Only the player that originally opened the
 * connection has the handle in its SlotMap, so the others no-op.
 *
 * One bridge per content-script world is enough. Each `<embed>` /
 * iframe SWF on a page lives in its own MAIN-world context with its own
 * copy of this module, so the player set we track is naturally scoped.
 */
// Structural type for the wasm RuffleHandle. We can't import the real
// class — it lives inside llflash-core's wasm-bindgen output and isn't
// part of the public TS surface. Method signatures match
// `dispatchRtmp*` exports in `web/src/lib.rs`.
interface RtmpPlayer {
    dispatchRtmpStatus(handle: bigint, code: string, level: string): void;
    dispatchRtmpCallResult(
        handle: bigint,
        txid: number,
        isError: boolean,
        bodyAmf: Uint8Array,
    ): void;
    dispatchRtmpServerCall(
        handle: bigint,
        method: string,
        argsAmf: Uint8Array,
    ): void;
}

interface InboundStatus {
    ev: "status";
    handle: number;
    code: string;
    level: string;
}
interface InboundCallResult {
    ev: "callResult";
    handle: number;
    txid: number;
    isError: boolean;
    bodyAmf: string;
}
interface InboundServerCall {
    ev: "serverCall";
    handle: number;
    method: string;
    argsAmf: string;
}
interface InboundReady {
    ev: "ready";
    version: string;
}
interface InboundLog {
    ev: "log";
    level: string;
    msg: string;
}
type Inbound =
    | InboundStatus
    | InboundCallResult
    | InboundServerCall
    | InboundReady
    | InboundLog;

const players: RtmpPlayer[] = [];

/**
 * Outbound callback. Builders register this via `setRtmpBridge`; wasm
 * invokes it inside the AVM `NetConnection.connect/call/close` hook.
 * The payload is already the shape the native messaging host expects.
 *
 * Two transport modes depending on where we're running:
 *   - Regular web page (llflash.js MAIN-world content script): postMessage
 *     to the ISOLATED-world content.ts, which owns the `chrome.runtime`
 *     port to the background service worker. MAIN-world content scripts
 *     don't have `chrome.runtime` access, so the relay is required.
 *   - Extension page (player.html in the SWF-takeover tab): open the
 *     `llflash-rtmp` port directly from this world — extension pages have
 *     `chrome.runtime` and there's no content script injected here.
 */
export function bridgeOut(msg: object): void {
    const port = ensureExtensionPort();
    if (port) {
        try {
            port.postMessage(msg);
        } catch (e) {
            console.warn("rtmp-bridge: extension-page port post failed", e);
        }
    } else {
        window.postMessage({ to: "llflash_rtmp_out", data: msg }, "*");
    }
}

// Cached port for the extension-page transport. Lazily opened on the first
// outbound message so we don't spawn a native host process for SWFs that
// never touch RTMP.
let extensionPort: chrome.runtime.Port | null = null;
function ensureExtensionPort(): chrome.runtime.Port | null {
    if (extensionPort) return extensionPort;
    // MAIN-world scripts on regular pages don't have `chrome.runtime`.
    // Only extension pages (player.html, options.html, etc.) reach here.
    if (typeof chrome === "undefined" || !chrome.runtime?.id) return null;
    try {
        const port = chrome.runtime.connect({ name: "llflash-rtmp" });
        port.onMessage.addListener((ev) => dispatch(ev as Inbound));
        port.onDisconnect.addListener(() => {
            if (extensionPort === port) extensionPort = null;
        });
        extensionPort = port;
        return port;
    } catch (e) {
        console.warn("rtmp-bridge: failed to open extension-page port", e);
        return null;
    }
}

/**
 * Called from `inner.tsx` right after `builder.build()`. We just keep
 * the handle around — dispatching to it is the player's responsibility
 * via the wasm-side `dispatchRtmp*` methods.
 */
export function registerPlayer(player: object): void {
    players.push(player as RtmpPlayer);
}

/**
 * Called from `inner.tsx destroy()` right before the wasm-side
 * `INSTANCES.remove()`. Splices this player out of the dispatch list
 * so inbound RTMP events from still-open native connections don't
 * fan out to a dead handle. Identity comparison: the caller passes
 * the same `RuffleHandle` JS object that was registered.
 */
export function unregisterPlayer(player: object): void {
    const idx = players.indexOf(player as RtmpPlayer);
    if (idx !== -1) {
        players.splice(idx, 1);
    }
}

function atobToBytes(s: string): Uint8Array {
    // atob throws on non-base64 input. Tolerate empty / bad strings
    // gracefully — a malformed payload from the native side shouldn't
    // crash the extension main world.
    if (!s) {
        return new Uint8Array(0);
    }
    try {
        const bin = atob(s);
        const out = new Uint8Array(bin.length);
        for (let i = 0; i < bin.length; i++) {
            out[i] = bin.charCodeAt(i);
        }
        return out;
    } catch (e) {
        console.warn("rtmp-bridge: bad base64 payload, treating as empty", e);
        return new Uint8Array(0);
    }
}

function dispatch(ev: Inbound): void {
    if (ev.ev === "ready") {
        console.info(`llflash native rtmp host ready (v${ev.version})`);
        return;
    }
    if (ev.ev === "log") {
        const fn =
            ev.level === "error"
                ? console.error
                : ev.level === "warn"
                  ? console.warn
                  : console.info;
        fn(`[llflash-rtmp-host] ${ev.msg}`);
        return;
    }
    const handle = BigInt(ev.handle);
    for (const p of players) {
        try {
            switch (ev.ev) {
                case "status":
                    p.dispatchRtmpStatus(handle, ev.code, ev.level);
                    break;
                case "callResult":
                    p.dispatchRtmpCallResult(
                        handle,
                        ev.txid,
                        ev.isError,
                        atobToBytes(ev.bodyAmf),
                    );
                    break;
                case "serverCall":
                    p.dispatchRtmpServerCall(
                        handle,
                        ev.method,
                        atobToBytes(ev.argsAmf),
                    );
                    break;
            }
        } catch (e) {
            // Player may have been destroyed; tolerate stale references.
            console.warn("rtmp-bridge: dispatch threw on a player", e);
        }
    }
}

window.addEventListener("message", (event) => {
    if (event.source !== window || !event.data) {
        return;
    }
    if (event.data.to !== "llflash_rtmp_in") {
        return;
    }
    const inbound = event.data.data as Inbound | undefined;
    if (!inbound || typeof (inbound as { ev?: unknown }).ev !== "string") {
        return;
    }
    dispatch(inbound);
});
