package flash.net {
    import __ruffle__.stub_method;

    import flash.events.EventDispatcher;
    import flash.net.URLRequest;

    public class URLLoader extends EventDispatcher {
        [Llflash(NativeAccessible)]
        public var data:*;

        [Llflash(NativeAccessible)]
        public var dataFormat:String = "text";

        [Llflash(NativeAccessible)]
        public var bytesLoaded:uint;

        [Llflash(NativeAccessible)]
        public var bytesTotal:uint;

        public function URLLoader(request:URLRequest = null) {
            if (request != null) {
                this.load(request);
            }
        }

        public native function load(request:URLRequest):void;

        public function close():void {
            stub_method("flash.net.URLLoader", "close");
        }
    }
}
