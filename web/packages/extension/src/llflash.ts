import { Setup, setCurrentScriptURL } from "llflash-core";
import { Message } from "./messages";
import { bridgeOut, registerPlayer } from "./rtmp-bridge";

/**
 *
 * This script runs in the MAIN ExecutionWorld for the following reasons:
 *
 * 1. On Chrome, you are explicitly banned from registering custom elements.
 * 2. On Firefox, you can register custom elements but they can't expose any
 *    useful API surface, and can't even see their own methods.
 *
 */

// Current message ID to be included in openInNewTab
let currentMessageId: string | null = null;

function handleMessage(message: Message) {
    switch (message.type) {
        case "load": {
            const publicPath = new URL(".", message.publicPath);
            if (publicPath.protocol.includes("extension")) {
                __webpack_public_path__ = publicPath.href;
            }
            if (window.RufflePlayer === undefined) {
                window.RufflePlayer = {};
            }
            if (window.RufflePlayer.config === undefined) {
                window.RufflePlayer.config = {};
            }
            window.RufflePlayer.config = {
                ...message.config,
                ...window.RufflePlayer.config,
                openInNewTab,
                // RTMP plumbing — let the wasm player offload RTMP
                // sockets to our native messaging host via the content
                // script relay. Both halves must be set together: the
                // bridge for outbound commands, the register callback
                // so inbound events can find this player.
                rtmpBridge: bridgeOut,
                rtmpRegister: registerPlayer,
            };
            setCurrentScriptURL(publicPath);
            Setup.installRuffle("extension");
            return {};
        }
        case "ping":
            // Ping back.
            return {};
        default:
            // Ignore unknown messages.
            return null;
    }
}

function openInNewTab(swf: URL): void {
    const message = {
        to: "llflash_content",
        index: null,
        id: currentMessageId,
        data: {
            type: "open_url_in_player",
            url: swf.toString(),
        },
    };
    window.postMessage(message, "*");
}

window.addEventListener("message", (event) => {
    // We only accept messages from ourselves.
    if (event.source !== window || !event.data) {
        return;
    }

    const { to, index, data, id } = event.data;
    if (to === "ruffle_page") {
        currentMessageId = id;
        const response = handleMessage(data);
        if (response) {
            const message = {
                to: "llflash_content",
                index,
                id,
                data: response,
            };
            window.postMessage(message, "*");
        }
    }
});
