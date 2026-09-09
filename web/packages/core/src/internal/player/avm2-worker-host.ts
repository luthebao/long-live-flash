import type {
    Avm2WorkerNumericId,
    ParentToWorkerMessage,
    WebWorkerBootstrap,
    WebWorkerCommand,
    WorkerToParentMessage,
    WorkerWireValue,
} from "./avm2-worker-protocol";
import { workerIdKey } from "./avm2-worker-protocol";

export interface Avm2WorkerHostCallbacks {
    workerStarted(workerId: Avm2WorkerNumericId): void;
    workerTerminated(workerId: Avm2WorkerNumericId): void;
    injectMessage(
        channelId: Avm2WorkerNumericId,
        value: WorkerWireValue,
    ): void;
    closeChannel(channelId: Avm2WorkerNumericId): void;
}

interface HostedWorker {
    worker: Worker;
    ownerWorkerId: Avm2WorkerNumericId | null;
}

export class Avm2WorkerHost {
    private readonly workers = new Map<string, HostedWorker>();

    public constructor(private readonly callbacks: Avm2WorkerHostCallbacks) {}

    public processCommands(
        commands: WebWorkerCommand[],
        sourceWorkerId: Avm2WorkerNumericId | null = null,
    ): void {
        for (const command of commands) {
            switch (command.type) {
                case "spawnWorker":
                    this.spawnWorker(command.bootstrap, sourceWorkerId);
                    break;
                case "sendMessage":
                    this.routeMessage(
                        command.receiverWorkerId,
                        command.channelId,
                        command.value,
                    );
                    break;
                case "closeChannel":
                    this.routeChannelClose(
                        command.receiverWorkerId,
                        command.channelId,
                    );
                    break;
                case "terminateWorker":
                    this.terminateWorker(command.workerId, true);
                    break;
            }
        }
    }

    public terminateAll(): void {
        for (const hosted of this.workers.values()) {
            hosted.worker.terminate();
        }
        this.workers.clear();
    }

    private spawnWorker(
        bootstrap: WebWorkerBootstrap,
        ownerWorkerId: Avm2WorkerNumericId | null,
    ): void {
        const key = workerIdKey(bootstrap.workerId);
        if (this.workers.has(key)) {
            console.warn(`AVM2 Worker ${key} is already running`);
            return;
        }

        let worker: Worker;
        try {
            worker = new Worker(
                new URL("./avm2-worker-entry.js", import.meta.url),
                {
                    type: "module",
                    name: `llflash-avm2-worker-${key}`,
                },
            );
        } catch (error) {
            console.error(`Unable to create AVM2 Worker ${key}:`, error);
            this.notifyWorkerTerminated(bootstrap.workerId, ownerWorkerId);
            return;
        }

        const hosted: HostedWorker = { worker, ownerWorkerId };
        this.workers.set(key, hosted);

        worker.onmessage = (event: MessageEvent<WorkerToParentMessage>) => {
            this.handleWorkerMessage(event.data, hosted);
        };
        worker.onerror = (event: ErrorEvent) => {
            console.error(
                `AVM2 Worker ${key} failed: ${event.message || "unknown error"}`,
            );
            this.terminateWorker(bootstrap.workerId, true);
        };

        const message: ParentToWorkerMessage = {
            type: "bootstrap",
            bootstrap,
        };
        worker.postMessage(message);
    }

    private handleWorkerMessage(
        message: WorkerToParentMessage,
        hosted: HostedWorker,
    ): void {
        switch (message.type) {
            case "ready":
                this.notifyWorkerStarted(message.workerId, hosted.ownerWorkerId);
                break;
            case "commands":
                this.processCommands(message.commands, message.workerId);
                break;
            case "error":
                console.error(
                    `AVM2 Worker ${workerIdKey(message.workerId)}: ${message.message}`,
                );
                this.terminateWorker(message.workerId, true);
                break;
        }
    }

    private routeMessage(
        receiverWorkerId: Avm2WorkerNumericId,
        channelId: Avm2WorkerNumericId,
        value: WorkerWireValue,
    ): void {
        if (workerIdKey(receiverWorkerId) === "0") {
            this.callbacks.injectMessage(channelId, value);
            return;
        }

        const target = this.workers.get(workerIdKey(receiverWorkerId));
        if (!target) {
            console.warn(
                `Dropping AVM2 Worker message for missing worker ${workerIdKey(receiverWorkerId)}`,
            );
            return;
        }

        const message: ParentToWorkerMessage = {
            type: "message",
            channelId,
            value,
        };
        target.worker.postMessage(message);
    }

    private routeChannelClose(
        receiverWorkerId: Avm2WorkerNumericId,
        channelId: Avm2WorkerNumericId,
    ): void {
        if (workerIdKey(receiverWorkerId) === "0") {
            this.callbacks.closeChannel(channelId);
            return;
        }

        const target = this.workers.get(workerIdKey(receiverWorkerId));
        if (!target) {
            return;
        }

        const message: ParentToWorkerMessage = {
            type: "closeChannel",
            channelId,
        };
        target.worker.postMessage(message);
    }

    private terminateWorker(
        workerId: Avm2WorkerNumericId,
        notifyOwner: boolean,
    ): void {
        const key = workerIdKey(workerId);
        const hosted = this.workers.get(key);
        if (!hosted) {
            return;
        }

        hosted.worker.terminate();
        this.workers.delete(key);
        if (notifyOwner) {
            this.notifyWorkerTerminated(workerId, hosted.ownerWorkerId);
        }
    }

    private notifyWorkerStarted(
        workerId: Avm2WorkerNumericId,
        ownerWorkerId: Avm2WorkerNumericId | null,
    ): void {
        if (ownerWorkerId === null || workerIdKey(ownerWorkerId) === "0") {
            this.callbacks.workerStarted(workerId);
            return;
        }

        const owner = this.workers.get(workerIdKey(ownerWorkerId));
        if (!owner) {
            return;
        }
        const message: ParentToWorkerMessage = {
            type: "workerStarted",
            workerId,
        };
        owner.worker.postMessage(message);
    }

    private notifyWorkerTerminated(
        workerId: Avm2WorkerNumericId,
        ownerWorkerId: Avm2WorkerNumericId | null,
    ): void {
        if (ownerWorkerId === null || workerIdKey(ownerWorkerId) === "0") {
            this.callbacks.workerTerminated(workerId);
            return;
        }

        const owner = this.workers.get(workerIdKey(ownerWorkerId));
        if (!owner) {
            return;
        }
        const message: ParentToWorkerMessage = {
            type: "workerTerminated",
            workerId,
        };
        owner.worker.postMessage(message);
    }
}
