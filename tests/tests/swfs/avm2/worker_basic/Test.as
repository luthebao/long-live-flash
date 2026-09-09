package {
    import flash.display.Sprite;
    import flash.events.Event;
    import flash.system.MessageChannel;
    import flash.system.Worker;
    import flash.system.WorkerDomain;

    public class Test extends Sprite {
        private var background:Worker;
        private var toBackground:MessageChannel;
        private var toMain:MessageChannel;

        public function Test() {
            var supported:Boolean = Worker.isSupported;
            trace("supported=" + supported);
            trace("domainSupported=" + WorkerDomain.isSupported);
            if (!supported) {
                return;
            }

            if (Worker.current.isPrimordial) {
                trace("primordial=" + Worker.current.isPrimordial);

                background = WorkerDomain.current.createWorker(loaderInfo.bytes);
                toBackground = Worker.current.createMessageChannel(background);
                toMain = background.createMessageChannel(Worker.current);
                background.setSharedProperty("toBackground", toBackground);
                background.setSharedProperty("toMain", toMain);
                toMain.addEventListener(Event.CHANNEL_MESSAGE, onWorkerMessage);
                background.addEventListener(Event.WORKER_STATE, onWorkerState);
                background.start();
                toBackground.send({ value: 21, label: "worker" });
            } else {
                var incoming:MessageChannel = Worker.current.getSharedProperty("toBackground") as MessageChannel;
                var outgoing:MessageChannel = Worker.current.getSharedProperty("toMain") as MessageChannel;
                var payload:Object = incoming.receive(true);
                outgoing.send({ result: payload.value * 2, label: payload.label + "-ok" });
            }
        }

        private function onWorkerState(event:Event):void {
            trace("workerState=" + background.state);
        }

        private function onWorkerMessage(event:Event):void {
            var result:Object = toMain.receive();
            trace("result=" + result.result);
            trace("label=" + result.label);
            trace("messageAvailable=" + toMain.messageAvailable);
            trace("terminate=" + background.terminate());
        }
    }
}
