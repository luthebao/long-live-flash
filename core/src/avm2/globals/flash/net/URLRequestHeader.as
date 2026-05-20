package flash.net {
    public final class URLRequestHeader {
        [Llflash(NativeAccessible)]
        public var name:String;

        [Llflash(NativeAccessible)]
        public var value:String;

        public function URLRequestHeader(name:String = "", value:String = "") {
            this.name = name;
            this.value = value;
        }
    }
}
