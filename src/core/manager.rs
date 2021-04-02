use core::panic;
use std::{collections::{HashMap, VecDeque}, sync::mpsc::{self, Receiver, SendError, Sender}, usize};
use super::{socket::{SocketID, SocketState, Socket}, udp_utils::PacketCmd};
use super::packet::TransportPacket;
use super::udp_utils;

struct SocketContents {
    state: SocketState,
    // we only need one of the bufs
    send_buf: Option<VecDeque<u8>>,
    recv_buf: Option<VecDeque<u8>>,
    ret_sender: Sender<TaskRet>,
    // for server socket
    backlog_que: Option<VecDeque<SocketID>>, // what should we store here?
}

impl SocketContents {
    pub fn new (ret_sender: Sender<TaskRet>) -> Self {
        SocketContents {
            state: SocketState::CLOSED, send_buf: None, recv_buf: None, ret_sender, backlog_que: None
        }
    }

    pub fn send_ret (&self, ret: TaskRet) -> Result<(), SendError<TaskRet>> {
        self.ret_sender.send(ret)?;
        return Ok(())
    }
}

// Task Message from socket api to manager and from timer!
#[derive(Clone)]
pub enum TaskMsg {
    // (enum also can hold args)
    // ==== for socket API
    New(String), // (local_addr)
    Bind(SocketID, u8), // (sock_id, local_port)
    Listen(SocketID, u32), // (sock_id, backlog)
    Accept(),
    Connect(SocketID, String, u8),
    // ==== for UDP packet parsing
    // a new packet is received
    OnReceive(TransportPacket),
    // ==== for timer callback
    SYNTimeOut(), // timer send msg of time out
}

// return values for socket task
pub enum TaskRet {
    New(Result<SocketID, ()>), // return the assigned socket ID
    Bind(Result<SocketID, isize>), // return the updated sock ID
    Listen(Result<(), isize>),
    Accept(),
    Connect(Result<SocketID, isize>), // return the updated sock ID
}

pub struct SocketManager {
    socket_map: HashMap<SocketID, SocketContents>, // use hashmap to make recv packet mapping quick
    // task_queue: mpsc::Receiver<TaskMsg> // this is a channel which is also a queue
    ports_alloc: [bool; 256],
}

impl SocketManager {
    const BuffferCap: usize = 1 << 10; // how many bytes the buffer can hold

    pub fn new () -> Self {
        SocketManager{socket_map: HashMap::new(), ports_alloc: [false; 256]}
    }

    // go over the current list of used ports and assign a free local port.
    // this will distinguish the new socket
    pub fn new_socket_id(&self, local_addr: String) -> Result<SocketID, ()> {
        for port in 0..256 {
            if self.ports_alloc[port] == false {
                return Ok(SocketID::new(local_addr, port as u8));
            }
        }
        return Err(());
    }

    /**
     * Start this TCP manager
     * the entry point of manager thread
     */
    pub fn start(&mut self, task_sender: mpsc::Sender<TaskMsg>, task_queue: mpsc::Receiver<TaskMsg> , ret_channel_sender: mpsc::Sender<mpsc::Receiver<TaskRet>>) {
        // ==== start udp loops ====
        let (udp_send, udp_recv) = mpsc::channel::<PacketCmd>();
        udp_utils::start_loops(udp_recv, task_sender.clone());

        // handle tasks
        for task in task_queue {
            match task {
                TaskMsg::New(local_addr) => {
                    // create the channel for ret values, one for each socket
                    let (ret_send, ret_recv) = mpsc::channel::<TaskRet>();
                    ret_channel_sender.send(ret_recv).unwrap();

                    // store the ret sender inside the socket contents
                    let sock_content = SocketContents::new(ret_send);
                    // alloc a new sock id
                    if let Ok(sock_id) = self.new_socket_id(local_addr) {
                        sock_content.send_ret(TaskRet::New(Ok(sock_id.clone()))).unwrap();
                        self.socket_map.insert(sock_id, sock_content);
                        println!("New(): new socket created!");
                    } else {
                        sock_content.send_ret(TaskRet::New(Err(()))).unwrap();
                    }
                },

                TaskMsg::Bind(mut id, port) => {
                    if self.ports_alloc[port as usize] == false {
                        if let Some(content) = self.socket_map.remove(&id){
                            id.local_port = port;
                            // return Ok
                            content.send_ret(TaskRet::Bind(Ok(id.clone()))).unwrap();
                            self.socket_map.insert(id, content);
                            println!("Bind(): local port {} binded.", port);
                        }
                        // incoherence
                        else {
                            panic!("wrong prot alloc!");
                        }
                    }
                    // if port is not available 
                    else {
                        if let Some(content) = self.socket_map.get(&id) {
                            content.send_ret(TaskRet::Bind(Err(-1))).unwrap();
                        }
                        else {
                            panic!("Bind(): Unknown socket id!");
                        }
                    }
                }

                TaskMsg::Listen(id, backlog) => {
                    let mut sock_content = self.socket_map.remove(&id).unwrap();
                    if sock_content.backlog_que.is_some() {
                        println!("Listen(): do not call listen twice!");
                        sock_content.send_ret(TaskRet::Listen(Err(-1))).unwrap();
                    }
                    else {
                        // switch to ==== LISTEN ==== state
                        sock_content.state = SocketState::LISTEN;
                        sock_content.backlog_que = Some(VecDeque::with_capacity(backlog as usize));
                        println!("Listen(): start listening ...");
                        sock_content.send_ret( TaskRet::Listen(Ok(())) ).unwrap();
                    }
                    // must put the entry back to hash map
                    self.socket_map.insert(id, sock_content);
                }

                TaskMsg::Connect(mut id, dest_addr, dest_port) => {
                    let mut sock_content = self.socket_map.remove(&id).unwrap();

                    // check current state.
                    if let SocketState::CLOSED = sock_content.state {
                        id.remote_addr = dest_addr.parse().unwrap();
                        id.remote_port = dest_port;
                        // Maybe we do not need an extra udp request queue? Ok...
                        // this send is non-blocking, it just puts the packet into the que
                        udp_send.send(PacketCmd::SYN(id.clone())).unwrap();
    
                        sock_content.state = SocketState::SYN_SENT;
                        sock_content.send_buf = Some(VecDeque::with_capacity(Self::BuffferCap));
                        sock_content.ret_sender.send(TaskRet::Connect(Ok(id.clone()))).unwrap();
                        self.socket_map.insert(id, sock_content);

                    }
                    else {
                        println!("Connect(): Wrong state, cant connect to remote!");
                        sock_content.ret_sender.send(TaskRet::Connect(Err(-1))).unwrap();
                    }


                }
                
                TaskMsg::OnReceive(packet) => {
                    self.handle_receive(packet);
                }
                _ => {}
            }
        }
        
    }

    // handler function when a packet is received.
    // when ACK is recevied , the data in buf will be sent. The window slides.
    fn handle_receive (&mut self, packet: TransportPacket) {
        println!("OnReceive(): Got a new packet!");
        
    }



}