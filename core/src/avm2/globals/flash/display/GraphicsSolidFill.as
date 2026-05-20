package flash.display {
    [API("662")]
    public final class GraphicsSolidFill implements IGraphicsFill, IGraphicsData {
        [Llflash(NativeAccessible)]
        public var alpha:Number = 1.0;

        [Llflash(NativeAccessible)]
        public var color:uint = 0;

        public function GraphicsSolidFill(color:uint = 0, alpha:Number = 1.0) {
            this.alpha = alpha;
            this.color = color;
        }
    }
}
