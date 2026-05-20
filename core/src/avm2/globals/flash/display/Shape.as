package flash.display {
    [Llflash(InstanceAllocator)]
    public class Shape extends DisplayObject {
        [Llflash(NativeAccessible)]
        private var _graphics:Graphics;

        public native function get graphics():Graphics;
    }
}
