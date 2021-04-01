use core::panic;
use std::{hash::Hash, net::Ipv4Addr};
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

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SocketID {
    pub local_addr: Ipv4Addr,
    pub remote_addr: Ipv4Addr,
    pub local_port: u8, // when user call the bind(), we do not know whether it is src or dest...
    pub remote_port: u8, // this can be either dest or src port, depending on whether server or cilent socket.
}

impl SocketID {
    pub fn new (local_addr: String, local_port: u8) -> Self {
        SocketID{
            local_addr: local_addr.parse().unwrap(), remote_addr: Ipv4Addr::new(127, 0, 0, 1),
            local_port, remote_port: 0
        }
    }
}

pub struct Socket {
    pub id: SocketID, // the user only need to know the ID, actual work will be done by manager.
    task_sender: Sender<TaskMsg>, // common channel to send tasks to socket manager.
    ret_recv: Receiver<TaskRet>, // individual channel to get return values.
}

/*
* The following are the socket APIs of TCP transport service.
* The only servers to provide the API. The buffers are managered by the SocketManager.
* All APIs are NON-BLOCKING.
*/
impl Socket {

    pub fn new (local_addr: String ,task_sender: Sender<TaskMsg>, ret_channel_recv: &Receiver<Receiver<TaskRet>>) -> Self {

        task_sender.send(TaskMsg::New(local_addr)).unwrap();
        let ret_recv = ret_channel_recv.recv().unwrap();

        if let TaskRet::New(Ok(sock_id)) = ret_recv.recv().unwrap() {
            Socket{id: sock_id, task_sender, ret_recv}
        }
        // if we are running out of ports..
        else {
            panic!("No ports available.");
        }

    }

    /**
     * Bind a socket to a local port
     *
     * @param localPort int local port number to bind the socket to
     * @return int 0 on success, -1 otherwise
     */
    pub fn bind(&mut self, local_port: u8) -> Result<(), isize> {

        self.task_sender.send(TaskMsg::Bind(self.id.clone(), local_port)).unwrap();

        if let TaskRet::Bind(ret) = self.ret_recv.recv().unwrap() {
            if let Ok(ret_id) = ret {
                self.id = ret_id;
                return Ok(());
            }
            else {
                return Err(-1);
            }
        } else {
            println!("Socket Bind(): Can not get ret value!");
            return Err(-1);
        }
    }

    /**
     * Listen for connections on a socket
     * the socket turns into a server socket
     * @param backlog int Maximum number of pending connections
     * @return int 0 on success, -1 otherwise
     */
    pub fn listen(&self, backlog: u32) -> Result<(), isize> {
        self.task_sender.send(TaskMsg::Listen(self.id.clone(), backlog)).unwrap();

        if let TaskRet::Listen(ret) = self.ret_recv.recv().unwrap() {
            return ret;
        } else {
            println!("Socket Listen(): Can not listen!");
            return Err(-1);
        }
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
    pub fn connect(&mut self, dest_addr: String, dest_port: u8) -> Result<(), isize> {
        self.task_sender.send(TaskMsg::Connect(self.id.clone(), dest_addr, dest_port)).unwrap();
        if let TaskRet::Connect(Ok(ret)) = self.ret_recv.recv().unwrap() {
            self.id = ret;
            return Ok(());
        } else {
            println!("Socket Connect(): Can not connect!");
            return Err(-1);
        }
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