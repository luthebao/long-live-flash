export type Avm2WorkerNumericId = bigint;

export interface SerializedWorkerWireValue {
    kind: "serialized";
    bytes: number[];
}

export interface WorkerHandleWireValue {
    kind: "worker";
    workerId: Avm2WorkerNumericId;
    primordial: boolean;
}

export interface MessageChannelWireValue {
    kind: "messageChannel";
    channelId: Avm2WorkerNumericId;
    senderWorkerId: Avm2WorkerNumericId;
    receiverWorkerId: Avm2WorkerNumericId;
}

export type WorkerWireValue =
    | SerializedWorkerWireValue
    | WorkerHandleWireValue
    | MessageChannelWireValue;

export interface WebWorkerSharedProperty {
    key: string;
    value: WorkerWireValue;
}

export interface WebWorkerLaunchConfig {
    playerVersion: number;
    playerRuntime: number;
    playerMode: number;
}

export interface WebWorkerBootstrap {
    workerId: Avm2WorkerNumericId;
    swfBytes: number[];
    config: WebWorkerLaunchConfig;
    sharedProperties: WebWorkerSharedProperty[];
}

export type WebWorkerCommand =
    | {
          type: "spawnWorker";
          bootstrap: WebWorkerBootstrap;
      }
    | {
          type: "sendMessage";
          channelId: Avm2WorkerNumericId;
          senderWorkerId: Avm2WorkerNumericId;
          receiverWorkerId: Avm2WorkerNumericId;
          value: WorkerWireValue;
      }
    | {
          type: "closeChannel";
          channelId: Avm2WorkerNumericId;
          senderWorkerId: Avm2WorkerNumericId;
          receiverWorkerId: Avm2WorkerNumericId;
      }
    | {
          type: "terminateWorker";
          workerId: Avm2WorkerNumericId;
      };

export type ParentToWorkerMessage =
    | {
          type: "bootstrap";
          bootstrap: WebWorkerBootstrap;
      }
    | {
          type: "message";
          channelId: Avm2WorkerNumericId;
          value: WorkerWireValue;
      }
    | {
          type: "closeChannel";
          channelId: Avm2WorkerNumericId;
      }
    | {
          type: "workerStarted";
          workerId: Avm2WorkerNumericId;
      }
    | {
          type: "workerTerminated";
          workerId: Avm2WorkerNumericId;
      };

export type WorkerToParentMessage =
    | {
          type: "ready";
          workerId: Avm2WorkerNumericId;
      }
    | {
          type: "commands";
          workerId: Avm2WorkerNumericId;
          commands: WebWorkerCommand[];
      }
    | {
          type: "error";
          workerId: Avm2WorkerNumericId;
          message: string;
      };

export function workerIdKey(id: Avm2WorkerNumericId): string {
    return id.toString();
}
