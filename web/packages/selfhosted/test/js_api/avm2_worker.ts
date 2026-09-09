import { expect } from "chai";
import { Player } from "llflash-core";
import { loadJsAPI, throwIfError } from "../utils.js";

describe("AVM2 Worker Web/WASM", () => {
    loadJsAPI();

    it("executes a background AVM2 isolate and round-trips MessageChannel data", async () => {
        const player = await browser.$("#llflash-player").getElement();

        await browser.execute((playerElement) => {
            const player = playerElement as Player.PlayerElement;
            player.__ruffle_log__ = [];
            player.ruffle().traceObserver = (message) => {
                player.__ruffle_log__.push(message);
                console.log(`[trace] ${message}`);
            };
        }, player);

        await browser.execute(async (playerElement) => {
            const response = await fetch("/avm2_worker_assets/Test.swf");
            if (!response.ok) {
                throw new Error(`Unable to load AVM2 Worker fixture: ${response.status}`);
            }

            const data = new Uint8Array(await response.arrayBuffer());
            if (
                data.length < 4 ||
                data[0] !== 0x46 ||
                data[1] !== 0x57 ||
                data[2] !== 0x53
            ) {
                throw new Error("AVM2 Worker fixture is not an uncompressed FWS file");
            }

            // The fixture is compiled with the repository's asc.jar. Raise only
            // the SWF header version so Flash 11.4 Worker APIs are visible.
            data[3] = 17;

            const player = playerElement as Player.PlayerElement;
            await player.ruffle().load({
                data,
                swfFileName: "worker_basic.swf",
            });
            player.ruffle().resume();
        }, player);

        await browser.waitUntil(
            async () => {
                const trace = await browser.execute(
                    (playerElement) =>
                        (playerElement as Player.PlayerElement).__ruffle_log__,
                    player,
                );
                return trace.includes("result=42") && trace.includes("terminate=true");
            },
            {
                timeout: 30000,
                timeoutMsg: "Expected AVM2 browser Worker round-trip to complete",
            },
        );

        await throwIfError(browser);

        const trace = await browser.execute(
            (playerElement) =>
                (playerElement as Player.PlayerElement).__ruffle_log__,
            player,
        );

        expect(trace).to.include("supported=true");
        expect(trace).to.include("domainSupported=true");
        expect(trace).to.include("primordial=true");
        expect(trace).to.include("workerState=running");
        expect(trace).to.include("result=42");
        expect(trace).to.include("label=worker-ok");
        expect(trace).to.include("messageAvailable=false");
        expect(trace).to.include("terminate=true");
    });
});
