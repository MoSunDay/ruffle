package flash.system {
    import flash.utils.ByteArray;
    import flash.system.Worker;

    [API("680")] // the docs say 682, that's wrong
    [Ruffle(Abstract)]
    public final class WorkerDomain {
        public static function get isSupported():Boolean {
            // Same-arena emulation (route A): see flash.system.Worker.
            return true;
        }

        public static native function get current():WorkerDomain;

        public native function createWorker(swf:ByteArray, giveAppPrivileges:Boolean = false):Worker;
    }
}
