use std::net::Ipv4Addr;
use std::sync::mpsc::{Sender, Receiver};
use std::sync::{Arc, Mutex};

use super::manager::{TaskMsg, TaskRet};

// TCP socket states
#[derive(PartialEq)]
pub enum SocketState {
    // protocol states
    CLOSED,
    LISTEN,
    SYN_SENT,
    ESTABLISHED,
    SHUTDOWN // close requested, FIN not sent (due to unsent data in queue)
}

pub struct SocketID {
    pub src_addr: Ipv4Addr,
    pub dest_addr: Ipv4Addr,
    pub local_port: u8, // when user call the bind(), we do not know whether it is src or dest...
    pub remote_port: u8, // this can be either dest or src port, depending on whether server or cilent socket.
}

impl SocketID {
    pub fn new () -> Self {
        SocketID{
            src_addr: Ipv4Addr::new(127, 0, 0, 1), dest_addr: Ipv4Addr::new(127, 0, 0, 1),
            local_port: 0, remote_port: 0
        }
    }
}

pub struct Socket {
    pub id: SocketID, // the user only need to know the ID, actual work will be done by manager.
    task_sender: Arc<Mutex<Sender<TaskMsg>>>,
    ret_recv: Receiver<TaskRet>,
}

/*
* The following are the socket APIs of TCP transport service.
* The only servers to provide the API. The buffers are managered by the SocketManager.
* All APIs are NON-BLOCKING.
*/
impl Socket {

    pub fn new (task_sender: Arc<Mutex<Sender<TaskMsg>>>, ret_channel_recv: Arc<Mutex<Receiver<Receiver<TaskRet>>>>) -> Self {

        task_sender.lock().unwrap().send(TaskMsg::New);
        let ret_recv = ret_channel_recv.lock().unwrap().recv().unwrap();

        Socket{id: SocketID::new(), task_sender, ret_recv}
    }

    /**
     * Bind a socket to a local port
     *
     * @param localPort int local port number to bind the socket to
     * @return int 0 on success, -1 otherwise
     */
    pub fn bind(&self, localPort: u8) -> Result<(), isize> {

        return Err(-1);
    }

    /**
     * Listen for connections on a socket
     * the socket turns into a server socket
     * @param backlog int Maximum number of pending connections
     * @return int 0 on success, -1 otherwise
     */
    pub fn listen(backlog: u32) -> Result<(), isize> {
        
        return Err(-1);
    }

    /**
     * Accept a connection on a socket
     * Non-blocking?
     * @return TCPSock The first established connection on the request queue
     */
    pub fn accept() -> Result<Self, isize> {

        return Err(-1); // no pending connection
    }

    /**
     * Initiate connection to a remote socket
     *
     * @param destAddr int Destination node address
     * @param destPort int Destination port
     * @return int 0 on success, -1 otherwise
     */
    pub fn connect(destAddr: String, destPort: u8) -> Result<(), isize> {
        return Err(-1);
    }

    /**
     * Initiate closure of a connection (graceful shutdown)
     */
    pub fn close() {
    }

    /**
     * Release a connection immediately (abortive shutdown)
     */
    pub fn release() {
    }

    /**
     * Write to the socket up to len bytes from the buffer buf starting at
     * position pos.
     *
     * @param buf byte[] the buffer to write from
     * @param pos int starting position in buffer
     * @param len int number of bytes to write
     * @return int on success, the number of bytes written, which may be smaller
     *             than len; on failure, -1
     */
    pub fn write(buf: Vec<u8>, pos: u32, len: u32) -> Result<(), isize> {
        return Err(-1);
    }

    /**
     * Read from the socket up to len bytes into the buffer buf starting at
     * position pos.
     *
     * @param buf byte[] the buffer
     * @param pos int starting position in buffer
     * @param len int number of bytes to read
     * @return int on success, the number of bytes read, which may be smaller
     *             than len; on failure, -1
     */
    pub fn read(buf: &mut Vec<u8>, pos: u32, len: u32) -> Result<(), isize> {
        return Err(-1);
    }

    // pub fn isConnectionPending(&self) -> bool {
    //     return self.state == SocketState::SYN_SENT;
    // }

    // pub fn isClosed(&self) -> bool {
    //     return self.state == SocketState::CLOSED;
    // }

    // pub fn isConnected(&self) -> bool {
    //     return self.state == SocketState::ESTABLISHED;
    // }

    // pub fn isClosurePending(&self) -> bool {
    //     return self.state == SocketState::SHUTDOWN;
    // }

    /*
     * End of socket API
     */
}