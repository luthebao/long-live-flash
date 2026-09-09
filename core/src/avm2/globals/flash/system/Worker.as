package flash.system {
    import flash.events.EventDispatcher;
    import flash.system.MessageChannel;

    [API("682")]
    [Llflash(Abstract)]
    public final class Worker extends EventDispatcher {
        public static native function get isSupported():Boolean;

        private static var _current:Worker;

        public static function get current():Worker {
            if (!_current) {
                _current = instantiateInternal();
            }

            return _current;
        }

        public native function get isPrimordial():Boolean;
        public native function get state():String;

        public native function createMessageChannel(receiver:Worker):MessageChannel;
        public native function setSharedProperty(key:String, value:*):void;
        public native function getSharedProperty(key:String):*;
        public native function start():void;
        public native function terminate():Boolean;

        private static native function instantiateInternal():Worker;
    }
}
