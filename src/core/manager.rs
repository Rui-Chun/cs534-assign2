use std::{collections::{HashMap, VecDeque}, sync::mpsc};
use std::net::Ipv4Addr;
use super::socket::{SocketID, SocketState, Socket};

struct SocketContents {
    state: SocketState,
    send_buf: Option<VecDeque<u8>>,
    recv_buf: Option<VecDeque<u8>>,

}

// Task Message from socket api to manager and from timer!
pub enum TaskMsg {
    New,
    Bind(u8), // enum also can hold args
    // and so on
}

// return values for socket task
pub enum TaskRet {
    New,
    Bind,
}

pub struct SocketManager {
    socket_map: HashMap<SocketID, SocketContents>, // use hashmap to make recv packet mapping quick
    // task_queue: mpsc::Receiver<TaskMsg> // this is a channel which is also a queue
}

impl SocketManager {

    pub fn new () -> Self {
        SocketManager{socket_map: HashMap::new()}
    }

    /**
     * Start this TCP manager
     * the entry point of manager thread
     */
    pub fn start(&mut self, task_queue: mpsc::Receiver<TaskMsg> , ret_channel_sender: mpsc::Sender<mpsc::Receiver<TaskRet>>) {

        for task in task_queue {
            match task {
                TaskMsg::New => {
                    let (ret_send, ret_recv) = mpsc::channel::<TaskRet>();
                    ret_channel_sender.send(ret_recv).unwrap();
                }
                _ => {}
            }
        }
        
    }



}