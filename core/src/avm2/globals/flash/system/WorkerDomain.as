package flash.system {
    import flash.utils.ByteArray;
    import flash.system.Worker;

    [API("680")]
    [Llflash(Abstract)]
    public final class WorkerDomain {
        public static native function get isSupported():Boolean;

        private static var _current:WorkerDomain;

        public static function get current():WorkerDomain {
            if (!_current) {
                _current = instantiateInternal();
            }

            return _current;
        }

        public native function createWorker(swf:ByteArray, giveAppPrivileges:Boolean = false):Worker;

        [API("684")]
        public native function listWorkers():Vector.<Worker>;

        private static native function instantiateInternal():WorkerDomain;
    }
}
