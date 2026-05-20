package flash.display {
    import flash.geom.Matrix;

    [API("662")]
    public final class GraphicsBitmapFill implements IGraphicsFill, IGraphicsData {
        [Llflash(NativeAccessible)]
        public var bitmapData:BitmapData;

        [Llflash(NativeAccessible)]
        public var matrix:Matrix;

        [Llflash(NativeAccessible)]
        public var repeat:Boolean;

        [Llflash(NativeAccessible)]
        public var smooth:Boolean;

        public function GraphicsBitmapFill(
            bitmapData:BitmapData = null,
            matrix:Matrix = null,
            repeat:Boolean = true,
            smooth:Boolean = false
        ) {
            this.bitmapData = bitmapData;
            this.matrix = matrix;
            this.repeat = repeat;
            this.smooth = smooth;
        }
    }
}
