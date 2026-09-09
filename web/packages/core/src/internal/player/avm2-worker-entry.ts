import {
    bulkMemory,
    referenceTypes,
    saturatedFloatToInt,
    signExtensions,
    simd,
} from "wasm-feature-detect";
import type { Avm2WorkerInstance as Avm2WorkerInstanceType } from "../../../dist/llflash_web_worker";
import type {
    ParentToWorkerMessage,
    WebWorkerCommand,
    WorkerToParentMessage,
} from "./avm2-worker-protocol";

let instance: Avm2WorkerInstanceType | null = null;
let currentWorkerId: bigint = 0n;
let tickTimer: ReturnType<typeof setTimeout> | null = null;
let initializing = false;
const pendingMessages: ParentToWorkerMessage[] = [];

async function loadWorkerRuntime(): Promise<
    typeof import("../../../dist/llflash_web_worker").Avm2WorkerInstance
> {
    const extensionsSupported = (
        await Promise.all([
            bulkMemory(),
            simd(),
            saturatedFloatToInt(),
            signExtensions(),
            referenceTypes(),
        ])
    ).every(Boolean);

    const module = await (extensionsSupported
        ? import("../../../dist/llflash_web_worker")
        : // @ts-expect-error replaced during the package build.
          import("../../../dist/%FALLBACK_WORKER_WASM%"));

    const wasmUrl = extensionsSupported
        ? new URL("../../../dist/llflash_web_worker_bg.wasm", import.meta.url)
        : new URL(
              "../../../dist/%FALLBACK_WORKER_WASM%_bg.wasm",
              import.meta.url,
          );
    const response = await fetch(wasmUrl);
    await module.default({ module_or_path: response });
    return module.Avm2WorkerInstance;
}

function post(message: WorkerToParentMessage): void {
    self.postMessage(message);
}

function flushCommands(): void {
    if (!instance) {
        return;
    }
    const commands = instance.takeCommands() as WebWorkerCommand[];
    if (commands.length === 0) {
        return;
    }
    post({
        type: "commands",
        workerId: currentWorkerId,
        commands,
    });
}

function scheduleTick(): void {
    if (!instance) {
        return;
    }
    const requested = instance.timeTilNextFrameMs();
    const delay = Number.isFinite(requested)
        ? Math.max(1, Math.min(requested, 10))
        : 4;
    tickTimer = setTimeout(runTick, delay);
}

function runTick(): void {
    tickTimer = null;
    if (!instance) {
        return;
    }
    try {
        instance.tick(performance.now());
        flushCommands();
        scheduleTick();
    } catch (error) {
        post({
            type: "error",
            workerId: currentWorkerId,
            message: error instanceof Error ? error.message : String(error),
        });
    }
}

async function bootstrap(message: Extract<ParentToWorkerMessage, { type: "bootstrap" }>) {
    if (initializing || instance) {
        return;
    }
    initializing = true;
    currentWorkerId = message.bootstrap.workerId;
    try {
        const Avm2WorkerInstance = await loadWorkerRuntime();
        instance = new Avm2WorkerInstance(message.bootstrap);
        post({ type: "ready", workerId: currentWorkerId });

        for (const pending of pendingMessages.splice(0)) {
            handleInitializedMessage(pending);
        }
        flushCommands();
        scheduleTick();
    } catch (error) {
        post({
            type: "error",
            workerId: currentWorkerId,
            message: error instanceof Error ? error.message : String(error),
        });
    } finally {
        initializing = false;
    }
}

function handleInitializedMessage(message: ParentToWorkerMessage): void {
    if (!instance) {
        pendingMessages.push(message);
        return;
    }

    switch (message.type) {
        case "bootstrap":
            break;
        case "message":
            instance.injectMessage(message.channelId, message.value);
            break;
        case "closeChannel":
            instance.closeChannel(message.channelId);
            break;
        case "workerStarted":
            instance.workerStarted(message.workerId);
            break;
        case "workerTerminated":
            instance.workerTerminated(message.workerId);
            break;
    }
}

self.onmessage = (event: MessageEvent<ParentToWorkerMessage>) => {
    const message = event.data;
    if (message.type === "bootstrap") {
        void bootstrap(message);
        return;
    }
    handleInitializedMessage(message);
};

self.addEventListener("close", () => {
    if (tickTimer !== null) {
        clearTimeout(tickTimer);
    }
});
