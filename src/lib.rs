pub mod core {
    pub mod manager;
    pub mod socket;
    mod packet;
    mod udp_loop;
}

pub mod app {
    mod client;
    mod server;
}


/* ====== Structures =====
 *  |-- Node.rs : create threds for server or client apps
 *  |-- manger.rs : a udp packet loop thread, a socket management/timer thread, probably task queues
 */