use core::panic;
use std::{hash::Hash, net::Ipv4Addr, usize};
use std::sync::mpsc::{Sender, Receiver};
use std::{thread, time};

use super::manager::{TaskMsg, TaskRet};

// TCP socket states
#[derive(PartialEq)]
#[allow(non_camel_case_types)]
/// transport socket states
pub enum SocketState {
    CLOSED,
    LISTEN,
    SYN_SENT,
    ESTABLISHED,
    /// We need to close, but still work to do. 
    /// When the receiver reveived FIN, but has still data not read by the user.
    SHUTDOWN,
    /// wait for all the packets to be acked, so that we can send FIN
    FIN_WAIT,
    /// a FIN has been sent
    FIN_SENT,
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
            // remote addr = 0.0.0.0 is used by server listening
            local_addr: local_addr.parse().unwrap(), remote_addr: Ipv4Addr::new(0, 0, 0, 0),
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

    /// init a new socket, called by user applications
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

    /// a new server connection, called by manager only
    pub fn new_coonection (sock_id: SocketID, task_sender: Sender<TaskMsg>, ret_recv: Receiver<TaskRet>) -> Self{
        Socket{id: sock_id, task_sender, ret_recv}
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
    pub fn accept(&self) -> Result<Self, isize> {
        self.task_sender.send(TaskMsg::Accept(self.id.clone())).unwrap();

        if let TaskRet::Accept(ret) = self.ret_recv.recv().unwrap() {
            return ret;
        } else {
            println!("Socket Accept(): Can not accept!");
            return Err(-1);
        }
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
     * it will consume the socket.
     */
    pub fn close(self) {
        self.task_sender.send(TaskMsg::Close(self.id)).unwrap();
    }

    /**
     * Release a connection immediately (abortive shutdown)
     * it will consume the socket.
     */
    pub fn release(self) {
        self.task_sender.send(TaskMsg::Release(self.id)).unwrap();
    }

    /**
     * Write to the socket up to len bytes from the buffer buf starting at position pos.
     *
     * @param buf byte[] the buffer to write from
     * @param pos int starting position in buffer
     * @param len int number of bytes to write
     * @return int on success, the number of bytes written, which may be smaller than len; on failure, -1
     */
    pub fn write(&self, buf: &Vec<u8>, pos: usize, len: usize) -> Result<usize, isize> {
        self.task_sender.send(TaskMsg::Write(self.id.clone(), buf.to_owned(), pos as u32, len as u32)).unwrap();

        if let TaskRet::Write(ret) = self.ret_recv.recv().unwrap() {
            return ret;
        } else {
            println!("Socket Write(): Can not write!");
            return Err(-1);
        }

    }

    pub fn write_all (&self, buf: &Vec<u8>) -> Result<(), isize> {
        let mut pos = 0;
        let mut len = buf.len();
        loop {
            let amount = self.write(buf, pos, len).unwrap();
            println!("Wrote {} bytes, left len = {}", amount, len);
            if amount == len {
                break;
            } else {
                len -= amount;
                pos += amount;
            }
            // wait for the sending
            thread::sleep(time::Duration::from_millis(100));
        }
        return Ok(());
    }

    /**
     * Read from the socket up to len bytes into the buffer buf starting at
     * position pos.
     *
     * @param len int number of bytes to read
     * @return return the data if successful, else return err code
     */
    pub fn read(&self, len: usize) -> Result<Vec<u8>, isize> {
        self.task_sender.send(TaskMsg::Read(self.id.clone(), len as u32)).unwrap();

        if let TaskRet::Read(ret) = self.ret_recv.recv().unwrap() {
            return ret;
        } else {
            println!("Socket Read(): Can not read!");
            return Err(-1);
        }
    }

    /// non-blocking read. keep trying until done.
    pub fn read_all(&self, amount: usize) -> Result<Vec<u8>, isize> {
        let mut recv_data = Vec::new();
        loop {
            recv_data.append(&mut self.read(amount).expect("Read failed"));
            if recv_data.len() == amount {
                break;
            }
            // wait for new data
            thread::sleep(time::Duration::from_millis(100));
        }
        return Ok(recv_data);
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