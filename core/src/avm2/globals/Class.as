package {
    [Llflash(InstanceAllocator)]
    public final dynamic class Class {
        public function Class() {
            // Unreachable because InstanceAllocator always throws an error
        }

        [Llflash(FastCall)]
        public final native function get prototype():*;

        public static const length:int = 1;
    }
}
