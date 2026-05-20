import * as utils from "./utils";
import { Setup } from "llflash-core";

import type { Config, Player } from "llflash-core";

declare global {
    interface Navigator {
        /**
         * iPadOS sends a User-Agent string that appears to be from macOS.
         * navigator.standalone is not defined on macOS, so we use it for iPad detection.
         */
        standalone?: boolean;
    }
}

Setup.installRuffle("local");
const ruffle = (window.RufflePlayer as Setup.PublicAPI).newest()!;
let player: Player.PlayerElement;

const playerContainer = document.getElementById("player-container")!;
const overlay = document.getElementById("overlay")!;
const localFileInput = document.getElementById(
    "local-file",
)! as HTMLInputElement;
const localFileName = document.getElementById("local-file-name")!;
const toggleInfo = document.getElementById("toggle-info")!;
const reloadSwf = document.getElementById("reload-swf")!;
const infoContainer = document.getElementById("info-container")!;
const webFormSubmit = document.getElementById("web-form-submit")!;
const webURL = document.getElementById("web-url")! as HTMLInputElement;
const modal = document.getElementById("modal")! as HTMLDialogElement;
const closeModal = document.getElementById("close")! as HTMLButtonElement;
const grant = document.getElementById("grant")! as HTMLButtonElement;

// This is the base config always used by the extension player.
// It has the highest priority and its options cannot be overwritten.
//
// Canvas device-font rendering is used so that text drawn through Flash
// "device fonts" can render any glyph the host browser has (CJK, full
// Vietnamese, etc.). The embedded subset bundled with the wasm only
// covers Latin/Greek/Cyrillic — anything else logs "UTF-8 character is
// missing" and renders as tofu.
const baseExtensionConfig = {
    letterbox: "on" as Config.Letterbox,
    forceScale: true,
    forceAlign: true,
    showSwfDownload: true,
    deviceFontRenderer: "canvas" as Config.DeviceFontRenderer,
};

const swfToFlashVersion: { [key: number]: string } = {
    1: "1",
    2: "2",
    3: "3",
    4: "4",
    5: "5",
    6: "6",
    7: "7",
    8: "8",
    9: "9.0",
    10: "10.0/10.1",
    11: "10.2",
    12: "10.3",
    13: "11.0",
    14: "11.1",
    15: "11.2",
    16: "11.3",
    17: "11.4",
    18: "11.5",
    19: "11.6",
    20: "11.7",
    21: "11.8",
    22: "11.9",
    23: "12",
    24: "13",
    25: "14",
    26: "15",
    27: "16",
    28: "17",
    29: "18",
    30: "19",
    31: "20",
    32: "21",
    33: "22",
    34: "23",
    35: "24",
    36: "25",
    37: "26",
    38: "27",
    39: "28",
    40: "29",
    41: "30",
    42: "31",
    43: "32",
};

function unload() {
    if (player) {
        player.remove();
        document.querySelectorAll("span.metadata").forEach((el) => {
            el.textContent = "Loading";
        });
        document.getElementById("backgroundColor")!.style.backgroundColor =
            "white";
    }
}

async function load(
    options: string | Config.DataLoadOptions | Config.URLLoadOptions,
) {
    unload();
    player = ruffle.createPlayer();
    player.id = "player";
    playerContainer.append(player);
    const url =
        typeof options === "string"
            ? options
            : "url" in options
              ? options["url"]
              : undefined;
    let origin;
    try {
        origin = url ? new URL(url).origin + "/" : url;
    } catch {
        // Ignore
    }
    const hostPermissionsForSpecifiedTab =
        await utils.hasHostPermissionForSpecifiedTab(origin);
    if (origin && !hostPermissionsForSpecifiedTab) {
        const result = await showModal(origin);
        if (result === "") {
            const swfPlayerPermissions = utils.i18n.getMessage(
                "swf_player_permissions",
            );
            alert(swfPlayerPermissions);
            history.pushState("", document.title, window.location.pathname);
            return;
        }
    }
    await player.ruffle().load(options);
    player.addEventListener("loadedmetadata", () => {
        const metadata = player.ruffle().metadata;
        if (metadata) {
            for (const [key, value] of Object.entries(metadata)) {
                const metadataElement = document.getElementById(key);
                if (metadataElement) {
                    switch (key) {
                        case "backgroundColor":
                            metadataElement.style.backgroundColor =
                                value ?? "white";
                            break;
                        case "uncompressedLength":
                            metadataElement.textContent = `${value >> 10}Kb`;
                            break;
                        // @ts-expect-error This intentionally falls through to the default case
                        case "swfVersion":
                            document.getElementById(
                                "flashVersion",
                            )!.textContent = swfToFlashVersion[value] ?? null;
                        // falls through and executes the default case as well
                        default:
                            metadataElement.textContent = value;
                            break;
                    }
                }
            }
        }
    });
}

async function loadFile(file: File | undefined) {
    if (!file) {
        return;
    }
    if (file.name) {
        localFileName.textContent = file.name;
    }
    const data = await new Response(file).arrayBuffer();
    const options = await utils.getExplicitOptions();
    load({
        ...options,
        data: data,
        swfFileName: file.name,
        ...baseExtensionConfig,
    });
    history.pushState("", document.title, window.location.pathname);
}

localFileInput.addEventListener("change", (event) => {
    const eventTarget = event.target as HTMLInputElement;
    if (
        eventTarget?.files &&
        eventTarget?.files.length > 0 &&
        eventTarget.files[0]
    ) {
        loadFile(eventTarget.files[0]);
    }
});

playerContainer.addEventListener("dragenter", (event) => {
    event.stopPropagation();
    event.preventDefault();
});
playerContainer.addEventListener("dragleave", (event) => {
    event.stopPropagation();
    event.preventDefault();
    overlay.classList.remove("drag");
});
playerContainer.addEventListener("dragover", (event) => {
    event.stopPropagation();
    event.preventDefault();
    overlay.classList.add("drag");
});
playerContainer.addEventListener("drop", (event) => {
    event.stopPropagation();
    event.preventDefault();
    overlay.classList.remove("drag");
    if (event.dataTransfer) {
        localFileInput.files = event.dataTransfer.files;
        loadFile(event.dataTransfer.files[0]);
    }
});
localFileInput.addEventListener("dragleave", (event) => {
    event.stopPropagation();
    event.preventDefault();
    overlay.classList.remove("drag");
});
localFileInput.addEventListener("dragover", (event) => {
    event.stopPropagation();
    event.preventDefault();
    overlay.classList.add("drag");
});
localFileInput.addEventListener("drop", (event) => {
    event.stopPropagation();
    event.preventDefault();
    overlay.classList.remove("drag");
    if (event.dataTransfer) {
        localFileInput.files = event.dataTransfer.files;
        loadFile(event.dataTransfer.files[0]);
    }
});

toggleInfo.addEventListener("click", () => {
    if (infoContainer.style.display === "none") {
        infoContainer.style.display = "flex";
    } else {
        infoContainer.style.display = "none";
    }
});

reloadSwf.addEventListener("click", () => {
    if (player) {
        const confirmReload = confirm("Reload the current SWF?");
        if (confirmReload) {
            player.ruffle().reload();
        }
    }
});
function showModal(origin: string) {
    return new Promise((resolve, _reject) => {
        grant.textContent = "Grant permissions on " + origin;
        function grantClicked() {
            modal.close();
            utils.permissions
                .request({
                    origins: [origin],
                })
                .then((permissionsGranted) => {
                    if (permissionsGranted) {
                        resolve(origin);
                    } else {
                        resolve("");
                    }
                })
                .catch(() => {
                    resolve("");
                })
                .finally(() => {
                    closeModal.removeEventListener("click", closeClicked);
                });
        }

        function closeClicked() {
            modal.close();
            resolve("");
            grant.removeEventListener("click", grantClicked);
        }

        grant.addEventListener("click", grantClicked, { once: true });
        closeModal.addEventListener("click", closeClicked, { once: true });
        modal.showModal();
    });
}

window.addEventListener("load", () => {
    if (
        navigator.userAgent.match(/iPad/i) ||
        navigator.userAgent.match(/iPhone/i) ||
        (navigator.platform === "MacIntel" &&
            typeof navigator.standalone !== "undefined")
    ) {
        localFileInput.removeAttribute("accept");
    }
    overlay.removeAttribute("hidden");
});

// Derive the value to advertise as `pageUrl` for a SWF opened in the
// standalone player tab. `window.location.href` would otherwise be
// `chrome-extension://EXT/player.html#...`, which RTMP servers reject on
// hotlink checks.
//
// TODO(learning): decide the policy. Three reasonable options:
//   1. Return `swfUrl` as-is — the SWF *is* the page in this tab.
//   2. Return the SWF's origin + "/" — pretend the SWF was embedded at
//      the host root. Matches what some hotlink checks expect.
//   3. Return the SWF's directory (everything up to the last "/") —
//      matches a typical embed page at the same depth as the SWF.
// Pick one and replace the body below.
function derivePageUrlForSwf(swfUrl: string): string {
    return swfUrl;
}

async function loadSwf(swfUrl: string) {
    try {
        const pathname = new URL(swfUrl).pathname;
        document.title = pathname.substring(pathname.lastIndexOf("/") + 1);
    } catch (_) {
        // Ignore URL parsing errors.
    }

    const options = await utils.getExplicitOptions();
    localFileName.textContent = document.title;
    localFileInput.value = "";
    load({
        ...options,
        url: swfUrl,
        base: swfUrl.substring(0, swfUrl.lastIndexOf("/") + 1),
        pageUrl: derivePageUrlForSwf(swfUrl),
        ...baseExtensionConfig,
    });
}

async function loadSwfFromHash() {
    const url = new URL(window.location.href);
    // Hash always starts with #, gotta slice that off
    const swfUrl = url.hash.length > 1 ? url.hash.slice(1) : null;
    if (swfUrl) {
        webURL.value = swfUrl;
        await loadSwf(swfUrl);
    }
}

window.addEventListener("pageshow", loadSwfFromHash);

window.addEventListener("hashchange", loadSwfFromHash);

window.addEventListener("DOMContentLoaded", () => {
    document
        .getElementById("local-file-label")!
        .addEventListener("click", () => {
            document.getElementById("local-file")!.click();
        });
    webFormSubmit.addEventListener("click", () => {
        if (webURL.value !== "") {
            window.location.hash = webURL.value;
        }
    });
    webURL.addEventListener("keydown", (event) =>
        event.key === "Enter" ? webFormSubmit.click() : undefined,
    );
});
