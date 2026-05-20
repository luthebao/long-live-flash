package flash.media {
    [Llflash(InstanceAllocator)]
    public final class SoundTransform {
        public function SoundTransform(vol:Number = 1, panning:Number = 0) {
            this.volume = vol;
            this.pan = panning;
        }

        [Llflash(FastCall)]
        public native function get leftToLeft():Number;
        [Llflash(FastCall)]
        public native function set leftToLeft(value:Number):void;

        [Llflash(FastCall)]
        public native function get leftToRight():Number;
        [Llflash(FastCall)]
        public native function set leftToRight(value:Number):void;

        [Llflash(FastCall)]
        public native function get rightToLeft():Number;
        [Llflash(FastCall)]
        public native function set rightToLeft(value:Number):void;

        [Llflash(FastCall)]
        public native function get rightToRight():Number;
        [Llflash(FastCall)]
        public native function set rightToRight(value:Number):void;

        [Llflash(FastCall)]
        public native function get volume():Number;
        [Llflash(FastCall)]
        public native function set volume(volume:Number):void;

        [Llflash(FastCall)]
        public native function get pan():Number;
        [Llflash(FastCall)]
        public native function set pan(value:Number):void;
    }
}
