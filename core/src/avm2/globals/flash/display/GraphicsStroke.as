package flash.display {
    [API("662")]
    public final class GraphicsStroke implements IGraphicsStroke, IGraphicsData {
        [Llflash(NativeAccessible)]
        private var _caps:String;

        [Llflash(NativeAccessible)]
        public var fill:IGraphicsFill;

        [Llflash(NativeAccessible)]
        private var _joints:String;

        [Llflash(NativeAccessible)]
        public var miterLimit:Number;

        [Llflash(NativeAccessible)]
        public var pixelHinting:Boolean;

        [Llflash(NativeAccessible)]
        private var _scaleMode:String;

        [Llflash(NativeAccessible)]
        public var thickness:Number;

        public function GraphicsStroke(
            thickness:Number = NaN,
            pixelHinting:Boolean = false,
            scaleMode:String = "normal",
            caps:String = "none",
            joints:String = "round",
            miterLimit:Number = 3.0,
            fill:IGraphicsFill = null
        ) {
            this.thickness = thickness;
            this.pixelHinting = pixelHinting;
            this.scaleMode = scaleMode;
            this.caps = caps;
            this.joints = joints;
            this.miterLimit = miterLimit;
            this.fill = fill;
        }

        public function get caps():String {
            return this._caps;
        }
        public function set caps(value:String):void {
            if (value != "none" && value != "square" && value != "round") {
                throw new ArgumentError("Error #2008: Parameter caps must be one of the accepted values.", 2008);
            } else {
                this._caps = value;
            }
        }

        public function get joints():String {
            return this._joints;
        }
        public function set joints(value:String):void {
            if (value != "miter" && value != "bevel" && value != "round") {
                throw new ArgumentError("Error #2008: Parameter joints must be one of the accepted values.", 2008);
            } else {
                this._joints = value;
            }
        }

        public function get scaleMode():String {
            return this._scaleMode;
        }
        public function set scaleMode(value:String):void {
            if (value != "none" && value != "horizontal" && value != "vertical" && value != "normal") {
                throw new ArgumentError("Error #2008: Parameter scaleMode must be one of the accepted values.", 2008);
            } else {
                this._scaleMode = value;
            }
        }
    }
}
