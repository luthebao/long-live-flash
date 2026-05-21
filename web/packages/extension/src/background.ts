import * as utils from "./utils";
import { isMessage } from "./messages";

/**
 * RTMP native-messaging bridge.
 *
 * Each content-script port opened with name `"llflash-rtmp"` triggers
 * `connectNative` to the per-machine host, paired with that port for
 * the rest of its lifetime. One native host process per content frame
 * keeps handle namespaces from colliding across tabs / iframes —
 * native shutdown happens automatically when either side disconnects.
 *
 * The native host name is hard-coded; it must match what
 * `native-host/install.sh` writes into the browser's
 * NativeMessagingHosts manifest directory.
 */
const RTMP_HOST = "com.longliveflash.rtmp_host";

chrome.runtime.onConnect.addListener((port) => {
    if (port.name !== "llflash-rtmp") {
        return;
    }

    // Chrome port IPC does NOT reliably queue messages whose `onMessage`
    // listener is registered asynchronously — the first user `connect`
    // arrives here before the toggle read + `connectNative` complete, and
    // would be dropped. So attach port listeners SYNCHRONOUSLY, buffer
    // commands until the native pipe is live, then drain in order.
    const buffered: unknown[] = [];
    let native: chrome.runtime.Port | null = null;
    let portClosed = false;

    port.onMessage.addListener((cmd) => {
        if (native) {
            try {
                native.postMessage(cmd);
            } catch (e) {
                console.warn("llflash-rtmp: failed to forward to native", e);
            }
        } else {
            buffered.push(cmd);
        }
    });
    port.onDisconnect.addListener(() => {
        portClosed = true;
        try {
            native?.disconnect();
        } catch {
            // ignore
        }
        native = null;
    });

    void (async () => {
        const { rtmpEnable } = await utils.getOptions();
        if (portClosed) {
            return;
        }
        if (!rtmpEnable) {
            try {
                port.postMessage({
                    ev: "log",
                    level: "info",
                    msg: "RTMP host is off. Enable it in the Llflash popup to allow native RTMP connections.",
                });
            } catch {
                // ignore
            }
            try {
                port.disconnect();
            } catch {
                // ignore
            }
            return;
        }

        let n: chrome.runtime.Port;
        try {
            n = chrome.runtime.connectNative(RTMP_HOST);
        } catch (e) {
            console.warn(
                `llflash-rtmp: failed to spawn native host '${RTMP_HOST}'`,
                e,
            );
            try {
                port.postMessage({
                    ev: "log",
                    level: "error",
                    msg: `failed to spawn native host '${RTMP_HOST}': ${e instanceof Error ? e.message : String(e)}`,
                });
            } catch {
                // ignore
            }
            try {
                port.disconnect();
            } catch {
                // ignore
            }
            return;
        }

        if (portClosed) {
            try {
                n.disconnect();
            } catch {
                // ignore
            }
            return;
        }

        n.onMessage.addListener((msg) => {
            try {
                port.postMessage(msg);
            } catch {
                // Content port already torn down.
            }
        });
        n.onDisconnect.addListener(() => {
            const err = chrome.runtime.lastError;
            if (err) {
                try {
                    port.postMessage({
                        ev: "log",
                        level: "error",
                        msg: `native host disconnected: ${err.message ?? "unknown error"}`,
                    });
                } catch {
                    // ignore
                }
            }
            try {
                port.disconnect();
            } catch {
                // ignore
            }
        });

        native = n;

        // Drain anything that arrived during the async setup gap.
        for (const cmd of buffered) {
            try {
                n.postMessage(cmd);
            } catch (e) {
                console.warn(
                    "llflash-rtmp: failed to forward buffered cmd",
                    e,
                );
            }
        }
        buffered.length = 0;
    })();
});

async function contentScriptRegistered() {
    const matchingScripts = await utils.scripting.getRegisteredContentScripts({
        ids: ["plugin-polyfill"],
    });
    return matchingScripts?.length > 0;
}

// Copied from https://github.com/w3c/webextensions/issues/638#issuecomment-2181124486
async function isHeaderConditionSupported() {
    let needCleanup = false;
    const ruleId = 4;
    try {
        // Throws synchronously if not supported.
        await utils.declarativeNetRequest.updateDynamicRules({
            addRules: [
                {
                    id: ruleId,
                    condition: { responseHeaders: [{ header: "whatever" }] },
                    action: {
                        type:
                            chrome.declarativeNetRequest.RuleActionType
                                ?.ALLOW ?? "allow",
                    },
                },
            ],
        });
        needCleanup = true;
    } catch {
        return false; // responseHeaders condition not supported.
    }
    // Chrome may recognize the properties but have the implementation behind a flag.
    // When the implementation is disabled, validation is skipped too.
    try {
        await utils.declarativeNetRequest.updateDynamicRules({
            removeRuleIds: [ruleId],
            addRules: [
                {
                    id: ruleId,
                    condition: { responseHeaders: [] },
                    action: {
                        type:
                            chrome.declarativeNetRequest.RuleActionType
                                ?.ALLOW ?? "allow",
                    },
                },
            ],
        });
        needCleanup = true;
        return false; // Validation skipped = feature disabled.
    } catch {
        return true; // Validation worked = feature enabled.
    } finally {
        if (needCleanup) {
            await utils.declarativeNetRequest.updateDynamicRules({
                removeRuleIds: [ruleId],
            });
        }
    }
}

async function enableSWFTakeover() {
    // Checks if the responseHeaders condition is supported and not behind a disabled flag.
    if (utils.declarativeNetRequest && (await isHeaderConditionSupported())) {
        const { ruffleEnable } = await utils.getOptions();
        if (ruffleEnable) {
            const playerPage = utils.runtime.getURL("/player.html");
            const rules = [
                {
                    id: 1,
                    action: {
                        type:
                            chrome.declarativeNetRequest.RuleActionType
                                ?.REDIRECT ?? "redirect",
                        redirect: { regexSubstitution: playerPage + "#\\0" },
                    },
                    condition: {
                        regexFilter: ".*",
                        responseHeaders: [
                            {
                                header: "content-type",
                                values: [
                                    "application/x-shockwave-flash",
                                    "application/futuresplash",
                                    "application/x-shockwave-flash2-preview",
                                    "application/vnd.adobe.flash.movie",
                                ],
                            },
                        ],
                        resourceTypes: [
                            chrome.declarativeNetRequest.ResourceType
                                ?.MAIN_FRAME ?? "main_frame",
                        ],
                    },
                },
                {
                    id: 2,
                    action: {
                        type:
                            chrome.declarativeNetRequest.RuleActionType
                                ?.REDIRECT ?? "redirect",
                        redirect: { regexSubstitution: playerPage + "#\\0" },
                    },
                    condition: {
                        regexFilter:
                            "^.*:\\/\\/.*\\/.*\\.s(?:wf|pl)(\\?.*|#.*|)$",
                        responseHeaders: [
                            {
                                header: "content-type",
                                values: [
                                    "application/octet-stream",
                                    "application/binary-stream",
                                    "",
                                ],
                            },
                        ],
                        resourceTypes: [
                            chrome.declarativeNetRequest.ResourceType
                                ?.MAIN_FRAME ?? "main_frame",
                        ],
                    },
                },
                {
                    id: 3,
                    action: {
                        type:
                            chrome.declarativeNetRequest.RuleActionType
                                ?.REDIRECT ?? "redirect",
                        redirect: { regexSubstitution: playerPage + "#\\0" },
                    },
                    condition: {
                        regexFilter:
                            "^.*:\\/\\/.*\\/.*\\.s(?:wf|pl)(\\?.*|#.*|)$",
                        excludedResponseHeaders: [{ header: "content-type" }],
                        resourceTypes: [
                            chrome.declarativeNetRequest.ResourceType
                                ?.MAIN_FRAME ?? "main_frame",
                        ],
                    },
                },
            ];
            await chrome.declarativeNetRequest.updateDynamicRules({
                removeRuleIds: [1, 2, 3],
                addRules: rules,
            });
        }
        utils.storage.sync.set({ responseHeadersUnsupported: false });
    } else {
        utils.storage.sync.set({ responseHeadersUnsupported: true });
    }
}

async function disableSWFTakeover() {
    if (utils.declarativeNetRequest && (await isHeaderConditionSupported())) {
        await utils.declarativeNetRequest.updateDynamicRules({
            removeRuleIds: [1, 2, 3],
        });
        utils.storage.sync.set({ responseHeadersUnsupported: false });
    } else {
        utils.storage.sync.set({ responseHeadersUnsupported: true });
    }
}

async function enable() {
    const { swfTakeover } = await utils.getOptions();
    if (swfTakeover) {
        await enableSWFTakeover();
    }
    if (
        !utils.scripting ||
        (utils.scripting.ExecutionWorld && !utils.scripting.ExecutionWorld.MAIN)
    ) {
        return;
    }
    if (!(await contentScriptRegistered())) {
        // Reuse the exclude_matches of dist/content.js in the manifest.
        const excludeMatches =
            utils.runtime.getManifest().content_scripts![0]!.exclude_matches!;
        await utils.scripting.registerContentScripts([
            {
                id: "ruffle",
                js: ["dist/llflash.js"],
                persistAcrossSessions: true,
                matches: ["<all_urls>"],
                excludeMatches,
                runAt: "document_start",
                allFrames: true,
                world: "MAIN",
            },
            {
                id: "plugin-polyfill",
                js: ["dist/pluginPolyfill.js"],
                persistAcrossSessions: true,
                matches: ["<all_urls>"],
                excludeMatches,
                runAt: "document_start",
                allFrames: true,
                world: "MAIN",
            },
            {
                id: "4399",
                matches: [
                    "*://www.4399.com/flash/*",
                    "https://my.4399.com/*",
                    "https://news.4399.com/qiu/",
                    "http://sjsj.4399.com/",
                ],
                js: ["dist/siteContentScript4399.js"],
                world: "MAIN",
                runAt: "document_start",
            },
        ]);
    }
}

async function disable() {
    if (
        !utils.scripting ||
        (utils.scripting.ExecutionWorld && !utils.scripting.ExecutionWorld.MAIN)
    ) {
        return;
    }
    if (await contentScriptRegistered()) {
        await utils.scripting.unregisterContentScripts({
            ids: ["ruffle", "plugin-polyfill", "4399"],
        });
    }
    await disableSWFTakeover();
}

async function onAdded(
    permissions:
        | browser.permissions.Permissions
        | chrome.permissions.Permissions,
) {
    if (
        permissions.origins &&
        permissions.origins.length >= 1 &&
        permissions.origins[0] !== "<all_urls>"
    ) {
        await utils.storage.sync.set({
            ["showReloadButton"]: true,
        });
    }
}

function onMessage(
    request: unknown,
    _sender: chrome.runtime.MessageSender,
    _sendResponse: (response: unknown) => void,
): void {
    if (isMessage(request)) {
        if (request.type === "open_url_in_player") {
            chrome.tabs.create({
                url: utils.runtime.getURL(`player.html#${request.url}`),
            });
        }
    }
}

(async () => {
    const { ruffleEnable } = await utils.getOptions();
    if (ruffleEnable) {
        await enable();
    }
})();

// Listeners must be registered synchronously at the top level,
// otherwise they won't be called in time when the service worker wakes up
if (chrome?.runtime && !chrome.runtime.onMessage.hasListener(onMessage)) {
    chrome.runtime.onMessage.addListener(onMessage);
}

utils.storage.onChanged.addListener(async (changes, namespace) => {
    if (namespace === "sync" && "ruffleEnable" in changes) {
        if (changes["ruffleEnable"]!.newValue) {
            await enable();
        } else {
            await disable();
        }
    }
    if (namespace === "sync" && "swfTakeover" in changes) {
        if (changes["swfTakeover"]!.newValue) {
            await enableSWFTakeover();
        } else {
            await disableSWFTakeover();
        }
    }
});

async function handleInstalled(details: chrome.runtime.InstalledDetails) {
    if (
        details.reason === chrome.runtime.OnInstalledReason.INSTALL &&
        !(await utils.hasAllUrlsPermission())
    ) {
        await utils.openOnboardPage();
    }
}

chrome.runtime.onInstalled.addListener(handleInstalled);
utils.permissions.onAdded.addListener(onAdded);
