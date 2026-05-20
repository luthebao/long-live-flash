package flash.filters {
    public class ConvolutionFilter extends BitmapFilter {
        [Llflash(NativeAccessible)]
        public var alpha:Number;

        [Llflash(NativeAccessible)]
        public var bias:Number;

        [Llflash(NativeAccessible)]
        public var clamp:Boolean;

        [Llflash(NativeAccessible)]
        public var color:uint;

        [Llflash(NativeAccessible)]
        public var divisor:Number;

        [Llflash(NativeAccessible)]
        public var matrix:Array;

        [Llflash(NativeAccessible)]
        public var matrixX:Number;

        [Llflash(NativeAccessible)]
        public var matrixY:Number;

        [Llflash(NativeAccessible)]
        public var preserveAlpha:Boolean;

        public function ConvolutionFilter(
            matrixX:Number = 0,
            matrixY:Number = 0,
            matrix:Array = null,
            divisor:Number = 1.0,
            bias:Number = 0.0,
            preserveAlpha:Boolean = true,
            clamp:Boolean = true,
            color:uint = 0,
            alpha:Number = 0.0
        ) {
            this.alpha = alpha;
            this.bias = bias;
            this.clamp = clamp;
            this.color = color;
            this.divisor = divisor;
            this.matrix = matrix;
            this.matrixX = matrixX;
            this.matrixY = matrixY;
            this.preserveAlpha = preserveAlpha;
        }

        override public function clone():BitmapFilter {
            return new ConvolutionFilter(
                this.matrixX,
                this.matrixY,
                this.matrixull,
                this.divisor,
                this.bias,
                this.preserveAlpharue,
                this.clamprue,
                this.color,
                this.alpha
            );
        }
    }
}
