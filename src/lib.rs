pub mod core {
    pub mod manager;
    pub mod socket;
    mod packet;
    pub mod udp_utils;
    pub mod timer;
}


/* ====== Structures =====
 *  |-- Node.rs : create threds for server or client apps
 *  |-- manger.rs : a udp packet loop thread, a socket management/timer thread, probably task queues
 */