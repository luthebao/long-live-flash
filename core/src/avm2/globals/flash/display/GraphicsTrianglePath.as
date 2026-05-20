package flash.display {
    [API("662")]
    public final class GraphicsTrianglePath implements IGraphicsPath, IGraphicsData {
        [Llflash(NativeAccessible)]
        private var _culling:String;

        [Llflash(NativeAccessible)]
        public var indices:Vector.<int>;

        [Llflash(NativeAccessible)]
        public var uvtData:Vector.<Number>;

        [Llflash(NativeAccessible)]
        public var vertices:Vector.<Number>;

        public function GraphicsTrianglePath(
            vertices:Vector.<Number> = null,
            indices:Vector.<int> = null,
            uvtData:Vector.<Number> = null,
            culling:String = "none"
        ) {
            this.culling = culling;
            this.indices = indices;
            this.uvtData = uvtData;
            this.vertices = vertices;
        }

        public function get culling():String {
            return this._culling;
        }
        public function set culling(value:String):void {
            // TODO do validation
            this._culling = value;
        }
    }
}
