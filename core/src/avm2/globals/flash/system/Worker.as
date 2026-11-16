package flash.system {
    import flash.events.EventDispatcher;
    import flash.system.MessageChannel;

    [API("682")]
    [Ruffle(Abstract)]
    public final class Worker extends EventDispatcher {
        public static function get isSupported():Boolean {
            // Same-arena emulation (route A): workers run on the main thread
            // in a child ApplicationDomain.
            return true;
        }

        // Returns the worker whose code is currently executing, or the main
        // worker when called from the root movie.
        public static native function get current():Worker;

        public native function createMessageChannel(receiver:Worker):MessageChannel;

        // Shared properties live in the same AVM2 arena and are shared by
        // reference (Flash semantics for shareable values).
        public native function setSharedProperty(key:String, value:*):void;
        public native function getSharedProperty(key:String):*;

        public native function get state():String;

        // Marks the worker running and queues its SWF for boot on the next
        // player tick. `workerState` is dispatched asynchronously, matching
        // Flash (listeners may run before the other worker is created).
        public native function start():void;

        public native function terminate():void;
    }
}
