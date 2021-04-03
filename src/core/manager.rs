use core::panic;
use std::{collections::{HashMap, VecDeque}, sync::mpsc::{self, Receiver, SendError, Sender}, net::Ipv4Addr, usize};
use super::{socket::{SocketID, SocketState, Socket}, udp_utils::PacketCmd};
use super::packet::{TransportPacket, TransType};
use super::udp_utils;
use rand::Rng;

struct SocketContents {
    state: SocketState,
    // we only need one of the bufs
    send_buf: Option<VecDeque<u8>>,
    recv_buf: Option<VecDeque<u8>>,
    ret_sender: Sender<TaskRet>,
    // for server socket
    backlog_que: Option<VecDeque<Socket>>, // what should we store here?

    // ==== sliding window pos ====
    /// the start of sliding window, the first byte that is not acked
    send_base: u32, 
    /// the start of range to be filled with new data
    send_next: u32,
    /// send window size
    send_wind: u32,

    /// the start of bytes to be read by user
    recv_base: u32,
    // the start of bytes to be filled
    recv_next: u32,
    /// recv window size
    recv_wind: u32,
    
}

impl SocketContents {
    pub fn new (ret_sender: Sender<TaskRet>) -> Self {
        SocketContents {
            state: SocketState::CLOSED, send_buf: None, recv_buf: None, ret_sender, backlog_que: None,
            send_base:0, send_next:0, send_wind: SocketManager::BuffferCap as u32, 
            recv_base:0, recv_next:0, recv_wind: SocketManager::BuffferCap as u32
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
    Accept(SocketID),
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
    Accept(Result<Socket, isize>),
    Connect(Result<SocketID, isize>), // return the updated sock ID
}

pub struct SocketManager {
    socket_map: HashMap<SocketID, SocketContents>, // use hashmap to make recv packet mapping quick
    task_send: Sender<TaskMsg>,
    task_queue: Receiver<TaskMsg>, // this is a channel which is also a queue
    ret_channel_send: Sender<Receiver<TaskRet>>,
    udp_send: Sender<PacketCmd>,
    ports_alloc: [bool; 256],
}

impl SocketManager {
    const BuffferCap: usize = 1 << 12; // how many bytes the buffer can hold

    pub fn new () -> (Self, Sender<TaskMsg>, Receiver<Receiver<TaskRet>>) {
        // the channel to send tasks of sockets
        let (task_send, task_queue) = mpsc::channel::<TaskMsg>();
        // the channel to send the return value channel, channel of channel...
        let (ret_channel_send, ret_channel_recv) = mpsc::channel::<mpsc::Receiver<TaskRet>>();
        
        // ==== start udp loops ====
        let (udp_send, udp_recv) = mpsc::channel::<PacketCmd>();
        udp_utils::start_loops(udp_recv, task_send.clone());

        (
            SocketManager{socket_map: HashMap::new(), task_send: task_send.clone(), task_queue, ret_channel_send, udp_send, ports_alloc: [false; 256]},
            task_send,
            ret_channel_recv,
        )
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
    pub fn start(&mut self) {

        // handle tasks
        loop {
            let task = self.task_queue.recv().unwrap();
            match task {
                TaskMsg::New(local_addr) => {
                    // create the channel for ret values, one for each socket
                    let (ret_send, ret_recv) = mpsc::channel::<TaskRet>();
                    self.ret_channel_send.send(ret_recv).unwrap();

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
                        let seq_num = rand::thread_rng().gen_range(0..2048);
                        self.udp_send.send(PacketCmd::SYN(id.clone(), seq_num)).unwrap();
                        // ========
                        // TODO: Setup timer for ACK timeout!
    
                        sock_content.state = SocketState::SYN_SENT;
                        sock_content.send_buf = Some(VecDeque::with_capacity(Self::BuffferCap));
                        // set up the window
                        sock_content.send_base = seq_num;
                        sock_content.send_next = seq_num + 1;

                        // send connect() ret value to the user socket.
                        sock_content.ret_sender.send(TaskRet::Connect(Ok(id.clone()))).unwrap();
                        self.socket_map.insert(id, sock_content);

                    }
                    else {
                        println!("Connect(): Wrong state, cant connect to remote!");
                        sock_content.ret_sender.send(TaskRet::Connect(Err(-1))).unwrap();
                    }

                }

                TaskMsg::Accept(sock_id) => self.handle_accept(sock_id),
                
                TaskMsg::OnReceive(packet) => self.handle_receive(packet),

                _ => {}
            }
        }
        
    }

    /// handler function when accept() API is called
    fn handle_accept (&mut self, sock_id: SocketID) {
        let sock_content = self.socket_map.get_mut(&sock_id).unwrap();

        // pop a incomming connection
        let backlog_que_ref = &mut sock_content.backlog_que;
        let new_sock_try = backlog_que_ref.as_mut().unwrap().pop_front();

        if let Some(sock) = new_sock_try {
            // return new socket
            sock_content.send_ret(TaskRet::Accept(Ok(sock))).unwrap();
        } else {
            // return err since on connection left
            sock_content.send_ret(TaskRet::Accept(Err(0))).unwrap();
        }

    }

    /// handler function when a packet is received.
    // when ACK is recevied , the data in buf will be sent. The window slides.
    fn handle_receive (&mut self, packet: TransportPacket) {
        println!("OnReceive(): Got a new packet!");
        let sock_id = packet.get_sock_id();
        let sock_content_ref: &mut SocketContents = 
            // try normal socket first
            if let Some(content) = self.socket_map.get_mut(&sock_id) {
                content
            } else {
                // try server socket
                let mut server_sock_id =  sock_id.clone();
                server_sock_id.remote_addr = Ipv4Addr::new(0, 0, 0, 0);
                server_sock_id.remote_port = 0;
                self.socket_map.get_mut(&server_sock_id).expect("Can not find socket in map!")
            };

        match packet.get_type() {
            TransType::SYN => {
                println!("OnReceive(): Got a SYN packet!");
                // what if we got a retransmited SYN? -> just update the socket list?
                // 1. if SYN is lost, the client will retransmit, no processing required,
                // 2. if ACK is lost, the client will retransmit. The packet will match the new connection socket,
                //    and since its state is not listen, nothing happens.
                if let SocketState::LISTEN = sock_content_ref.state{
                    // Send ACK for SYN
                    self.udp_send.send(PacketCmd::ACK(sock_id.clone(), Self::BuffferCap as u32, packet.get_seq_num()+1)).unwrap();
                    
                    // init a new socket for the incoming connection
                    // 1. new socket content
                    let (ret_send, ret_recv) =  mpsc::channel::<TaskRet>();
                    let mut new_sock_content = SocketContents::new(ret_send);
                    new_sock_content.state = SocketState::ESTABLISHED;
                    new_sock_content.recv_buf = Some(VecDeque::with_capacity(Self::BuffferCap));
                    // set up window
                    new_sock_content.recv_base = packet.get_seq_num()+1;
                    new_sock_content.recv_next = packet.get_seq_num()+1;

                    // 2. push socket API into the queue
                    let new_sock = Socket::new_coonection(sock_id.clone(), self.task_send.clone(), ret_recv);
                    let backlog_que_ref = &mut sock_content_ref.backlog_que;
                    backlog_que_ref.as_mut().unwrap().push_back(new_sock);

                    // 3. push new socket to map
                    println!("OnReceive(): New socket pushed to backlog!");
                    self.socket_map.insert(sock_id.clone(), new_sock_content);
                    
                    // do not need to put back server socket, since we use ref
                } else {
                    // SYN arrived at a wrong time
                    // do nothing...
                } 
            }
            TransType::ACK => {
                println!("OnReceive(): Got a ACK packet!");
                match sock_content_ref.state {
                    SocketState::SYN_SENT => {
                        // switch established state
                        sock_content_ref.state = SocketState::ESTABLISHED;
                        // move the window
                        sock_content_ref.send_base = packet.get_seq_num();
                        assert!(sock_content_ref.send_base <= sock_content_ref.send_next);
                        println!("OnReceive(): New connection established!");
                    }
                    SocketState::ESTABLISHED => {
                        // move the window
                        sock_content_ref.send_base = packet.get_seq_num();
                        assert!(sock_content_ref.send_base <= sock_content_ref.send_next);
                    }
                    _ => {
                        println!("This state is not handled yet.")
                    }
                }
                // TODO: ACK should trigger new sending
            }
            _ => {
                println!("OnReceive(): Undefined packet type!");
            }
        }
    }



}