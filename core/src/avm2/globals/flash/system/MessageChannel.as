package flash.system {
    import flash.events.EventDispatcher;

    [API("682")]
    [Llflash(Abstract)]
    public final class MessageChannel extends EventDispatcher {
        public native function send(arg:*, queueLimit:int = -1):void;
        public native function receive(blockUntilReceived:Boolean = false):*;
        public native function close():void;
        public native function get state():String;
        public native function get messageAvailable():Boolean;
    }
}
