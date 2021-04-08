use core::{panic};
use std::{cmp, collections::{HashMap, VecDeque}, net::Ipv4Addr, sync::mpsc::{self, Receiver, SendError, Sender}, thread, time::{Duration, Instant}, u32, usize};
use super::{socket::{SocketID, SocketState, Socket}, timer::{TimerCmd, TimerToken}, udp_utils::PacketCmd};
use super::packet::{self, TransportPacket, TransType};
use super::udp_utils;
use super::timer;
use rand::{Rng};

use std::io::prelude::*;
use std::fs::File;


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

    /// flow control, recv window left
    send_flow_ctl: usize,
    /// conjestion control, num of bytes
    send_cong_ctrl: usize,
    dup_ack_record: u32,
    dup_ack_num: u32,
    /// window counter, how many times we have been limited by the win
    win_counter: u32,

    /// the start of bytes to be read by user
    recv_base: u32,
    // the start of bytes to be filled
    recv_next: u32,
    /// recv window size
    recv_wind: u32,

    /// round trip time
    rtt: Duration,
    /// packet trans records
    /// seq_num -> (transmitting instance, (optinal) timer token)
    packet_times: HashMap<u32, (Instant, Option<TimerToken>)>,

    /// the file name of the packet log file
    log_file: Option<File>,
}

impl SocketContents {
    pub fn new (ret_sender: Sender<TaskRet>) -> Self {
        SocketContents {
            state: SocketState::CLOSED, send_buf: None, recv_buf: None, ret_sender, backlog_que: None,
            send_base:0, send_next:0, send_wind: SocketManager::BUFFER_CAP as u32, 
            send_flow_ctl: SocketManager::BUFFER_CAP,
            send_cong_ctrl: SocketManager::MSS * 100, // no slow start, starting from a high value
            dup_ack_record: 0, dup_ack_num: 0, win_counter: 1,
            recv_base:0, recv_next:0, recv_wind: SocketManager::BUFFER_CAP as u32,
            rtt: Duration::from_micros(100000), // 100 ms as the init value for time out
            packet_times: HashMap::new(),
            log_file: None,
        }
    }

    pub fn send_ret (&self, ret: TaskRet) -> Result<(), SendError<TaskRet>> {
        self.ret_sender.send(ret)?;
        return Ok(())
    }

    pub fn print_log (&mut self, sock_id: &SocketID, ch: &'static str) {
        if self.log_file.is_none() {
            let mut socket_type = String::from("listener");
            if self.send_buf.is_some() {
                socket_type = "sender".to_string();
            } else if self.recv_buf.is_some() {
                socket_type = "receiver".to_string();
            }
            let log_name = format!("./target/logs/Log-{}:{}-from-{}:{}-{}", sock_id.local_addr, sock_id.local_port, sock_id.remote_addr, sock_id.remote_port, socket_type);
            self.log_file = Some(File::create(log_name).unwrap());
        }

        self.log_file.as_mut().unwrap().write_all(ch.as_bytes()).unwrap();

    }
}

// Task Message from socket api to manager and from timer!
#[derive(Clone)]
pub enum TaskMsg {
    // (enum also can hold args)
    // ==== for socket API
    /// (local_addr)
    New(String),
    /// (sock_id, local_port)
    Bind(SocketID, u8),
    /// (sock_id, backlog)
    Listen(SocketID, u32),
    Accept(SocketID),
    /// (sock_id, dest_addr, dest_port)
    Connect(SocketID, String, u8),
    /// (sock_id, buf, pos, len)
    Write(SocketID, Vec<u8>, u32, u32),
    /// (sock_id, len)
    Read(SocketID, u32),
    /// (sock_id)
    Close(SocketID),
    Release(SocketID),

    // ==== UDP packets related
    // a new packet is received
    OnReceive(TransportPacket),
    /// schedule a sending task
    /// (sock_id, trans_type, seq_start, len, retrans_flag)
    SendNow(SocketID,TransType, u32, u32, bool),

}

// return values for socket task
pub enum TaskRet {
    New(Result<SocketID, ()>), // return the assigned socket ID
    Bind(Result<SocketID, isize>), // return the updated sock ID
    Listen(Result<(), isize>),
    Accept(Result<Socket, isize>),
    Connect(Result<SocketID, isize>), // return the updated sock ID
    Write(Result<usize, isize>), // return data left un-written
    Read(Result<Vec<u8>, isize>),
}

pub struct SocketManager {
    socket_map: HashMap<SocketID, SocketContents>, // use hashmap to make recv packet mapping quick
    task_send: Sender<TaskMsg>,
    task_queue: Receiver<TaskMsg>, // this is a channel which is also a queue
    ret_channel_send: Sender<Receiver<TaskRet>>,
    udp_send: Sender<PacketCmd>,
    ports_alloc: [bool; 256],
    // for retrans timer
    timer_send: Sender<TimerCmd>,
    ttoken_recv: Receiver<TimerToken>,
}

impl SocketManager {
    const BUFFER_CAP: usize = 4000; // how many bytes the buffer can hold
    const RTT_RATIO: f64 = 0.9; // the exp update ratio of rtt
    const TIMEOUT_MULTI: u32 = 3; // timeout  = multi * rtt
    const RTT_LOW_BOUND: u64 = 5; // ms
    const MSS: usize = TransportPacket::MAX_PAYLOAD_SIZE; // minimal segment size for congestion control

    pub fn new (local_addr: String) -> (Self, Sender<TaskMsg>, Receiver<Receiver<TaskRet>>) {
        // ===== init channels for inter thread comm =====
        // the channel to send tasks of sockets
        let (task_send, task_queue) = mpsc::channel::<TaskMsg>();
        // the channel to send the return value channel, channel of channel...
        let (ret_channel_send, ret_channel_recv) = mpsc::channel::<mpsc::Receiver<TaskRet>>();
        
        // ==== start udp loops ====
        let (udp_send, udp_recv) = mpsc::channel::<PacketCmd>();
        udp_utils::start_loops(local_addr, udp_recv, task_send.clone());

        // ==== start timer thread ====
        let (mut retrans_timer, timer_send, ttoken_recv) = timer::Timer::new(task_send.clone());
        thread::spawn( move || {
            retrans_timer.start();
        });

        (
            SocketManager{
                socket_map: HashMap::new(), task_send: task_send.clone(), task_queue, ret_channel_send, 
                udp_send, ports_alloc: [false; 256], timer_send, ttoken_recv
            },
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
                        // ===== send SYN
                        // this send is non-blocking, it just puts the packet into the que
                        let seq_num = rand::thread_rng().gen_range(0..2048);
                        self.task_send.send(TaskMsg::SendNow(id.clone(), TransType::SYN, seq_num, 0, false)).unwrap();

                        // switch socket state
                        sock_content.state = SocketState::SYN_SENT;
                        sock_content.send_buf = Some(VecDeque::with_capacity(Self::BUFFER_CAP));
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

                TaskMsg::Write(sock_id, buf, pos, len) => self.handle_write(sock_id, buf, pos, len),

                TaskMsg::Read(sock_id, len) => self.handle_read(sock_id, len),

                TaskMsg::Close(sock_id) => self.handle_close(sock_id),

                TaskMsg::Release(sock_id) => self.handle_release(sock_id),

                TaskMsg::SendNow(sock_id, ttype, seq_num, len, retrans) => 
                                self.handle_sendnow(sock_id, ttype, seq_num, len, retrans),
                
                TaskMsg::OnReceive(packet) => self.handle_receive(packet),

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

    /// handler function when write() API is called
    /// this function is non-blocking, only copy the buffer
    fn handle_write (&mut self, sock_id: SocketID, buf: Vec<u8>, pos: u32, len: u32) {
        let sock_content = self.socket_map.get_mut(&sock_id).unwrap();
        println!("Write(): send_base = {}, send_next = {}, len = {}", sock_content.send_base, sock_content.send_next, len);
        if sock_content.state != SocketState::ESTABLISHED {
            // send ret value
            sock_content.send_ret(TaskRet::Write(Err(-1))).unwrap();
        }

        // write send buf
        let to_write = cmp::min(sock_content.send_base + sock_content.send_wind - sock_content.send_next, len);
        sock_content.send_buf.as_mut().unwrap().extend(buf[pos as usize..(pos+to_write) as usize].iter());
        sock_content.send_next += to_write;

        // schedule a sending task
        if to_write > 0 {
            self.task_send.send(TaskMsg::SendNow(sock_id, TransType::DATA, sock_content.send_next - to_write, to_write, false)).unwrap();
        }

        // send ret value
        sock_content.send_ret(TaskRet::Write(Ok( to_write as usize ))).unwrap();

        assert!(sock_content.send_next - sock_content.send_base == sock_content.send_buf.as_mut().unwrap().len() as u32,
                "Send buf size does not fit!");
    }


    /// handler function when read API is called
    fn handle_read (&mut self, sock_id: SocketID, len: u32) {
        let sock_content = self.socket_map.get_mut(&sock_id).unwrap();
        println!("Read() : recv_base = {}, recv_next = {}, len = {}", sock_content.recv_base, sock_content.recv_next, len);
        // derive the byte num to read
        let buf_left = sock_content.recv_next - sock_content.recv_base;
        let to_read = cmp::min(buf_left, len) as usize;

        // send data read
        let buf_read: Vec<u8> = sock_content.recv_buf.as_mut().unwrap().drain(..to_read).collect();
        sock_content.send_ret(TaskRet::Read(Ok( buf_read ))).unwrap();

        // move window
        if sock_content.recv_next - sock_content.recv_base == sock_content.recv_wind  && to_read > 0 {
            // we must tell the client the newest window size !!
            println!("recv buf full and read called. wind = {}", to_read);
            self.udp_send.send(PacketCmd::ACK(sock_id.clone(), to_read as u32, sock_content.recv_next)).unwrap();
        }
        sock_content.recv_base += to_read as u32;
        assert!(sock_content.recv_base <= sock_content.recv_next);
        assert!(sock_content.recv_next - sock_content.recv_base == sock_content.recv_buf.as_ref().unwrap().len() as u32);
        assert!(sock_content.recv_next - sock_content.recv_base <= sock_content.recv_wind);
        
    }

    /// handler function when a packet is received.
    // the socke will stay open until all data are sent
    fn handle_close (&mut self, sock_id: SocketID) {
        let sock_content = self.socket_map.get_mut(&sock_id);

        if sock_content.is_none() {
            // it has been closeds
            return;
        }

        let sock_content = sock_content.unwrap();
        // find out this is a socket or client socket.
        match sock_content.state {
            SocketState::LISTEN => {
                // if a server listen socket, just free all resources.
                self.ports_alloc[sock_id.local_port as usize] = false;
                self.socket_map.remove(&sock_id).unwrap();
            }
            // we only need to send FIN, if a connection is established.
            SocketState::ESTABLISHED => {
                // if it is a client send socket
                if sock_content.send_buf.is_some() {

                    // check whether all data are acked
                    if sock_content.send_base == sock_content.send_next {
                        // send FIN and get into FIN_SENT
                        self.task_send.send(TaskMsg::SendNow(sock_id, TransType::FIN, sock_content.send_next, 0, false)).unwrap();
                        sock_content.state = SocketState::FIN_SENT;

                    } else {
                        // get into FIN_WAIT, wait for retransmission and ack
                        sock_content.state = SocketState::FIN_WAIT;
                    }

                } 
                else if sock_content.recv_buf.is_some() {
                    // if it is server recv socket

                    // send FIN, get into FIN_SENT state.
                    self.task_send.send(TaskMsg::SendNow(sock_id, TransType::FIN, sock_content.recv_next, 0, false)).unwrap();
                    sock_content.state = SocketState::FIN_SENT;
                }
                else {
                    // an undefined socket
                    panic!("trying to close a undefined socket!");
                }
            }
            // if close is called in SHUTDOWN state
            SocketState::SHUTDOWN => {
                self.ports_alloc[sock_id.local_port as usize] = false;
                self.socket_map.remove(&sock_id).unwrap();
            }
            _ => {
                self.ports_alloc[sock_id.local_port as usize] = false;
                self.socket_map.remove(&sock_id).unwrap();
            }
        }

    }

    /// handler for release task
    fn handle_release (&mut self, sock_id: SocketID) {
        // release should only free the resources and change state to closed or delete it?
        self.ports_alloc[sock_id.local_port as usize] = false;
        self.socket_map.remove(&sock_id);
    }

    /// handler for send packets including retransmitting packets
    fn handle_sendnow (&mut self, sock_id: SocketID, ttype: TransType, mut seq_num: u32, mut len: u32, retrans_flag: bool) {
        let sock_content = self.socket_map.get_mut(&sock_id);
        let sock_content = if sock_content.is_none() {
            return;
        } else {
            sock_content.unwrap()
        };
        println!("Sending packet {:?} , send_base = {}, send_next = {}, seq_num = {}, len = {}, retrans = {}", ttype, sock_content.send_base, sock_content.send_next, seq_num, len, retrans_flag);

        // check whether sending packet is still needed 
        if sock_content.send_buf.is_some() && sock_content.send_base > seq_num {
            // if it is only partly needed
            if seq_num + len > sock_content.send_base {
                // update len and start pos
                len = seq_num + len - sock_content.send_base;
                seq_num = sock_content.send_base;
            } else {
                // otherwise do not retransmit
                println!("Retrans cancel because low seq_num. ");
                // but we need to set up the new timer at send base ...

                // this packet_times is used for rtt estimation and timer cancelation, if not transmited yet.
                // we do not need update it.
                let time_entry = sock_content.packet_times.remove(&seq_num);
                // if it has been transmitted before
                if time_entry.is_some() {
                    self.timer_send.send(TimerCmd::New(
                        Instant::now() + sock_content.rtt * Self::TIMEOUT_MULTI, 
                        TaskMsg::SendNow(sock_id.clone(), ttype.clone(), sock_content.send_base, sock_content.send_next - sock_content.send_base, true)
                    )).unwrap();
                    let ttoken = self.ttoken_recv.recv().unwrap();
                    sock_content.packet_times.insert(seq_num, (time_entry.unwrap().0, Some(ttoken)));
                } else {
                    self.timer_send.send(TimerCmd::New(
                        Instant::now() + sock_content.rtt * Self::TIMEOUT_MULTI, 
                        TaskMsg::SendNow(sock_id.clone(), ttype.clone(), sock_content.send_base, sock_content.send_next - sock_content.send_base, false)
                    )).unwrap();
                }
                return;
            }
        }

        // if retransmit SYN
        if let TransType::SYN = ttype {
            self.udp_send.send(PacketCmd::SYN(sock_id.clone(), seq_num)).unwrap();
            sock_content.print_log(&sock_id, "S");
            if retrans_flag {
                println!("Retransmiting SYN !!!");
                sock_content.print_log(&sock_id, "!");
            }
            assert!(seq_num == sock_content.send_base);
            // set up new timer
            self.timer_send.send(TimerCmd::New(
                Instant::now() + sock_content.rtt * Self::TIMEOUT_MULTI, // time limit
                TaskMsg::SendNow(sock_id.clone(), ttype.clone(), seq_num, len, true) // timeour callback
            )).unwrap();
            // record timer token
            let ttoken = self.ttoken_recv.recv().unwrap();
            sock_content.packet_times.insert(seq_num, (Instant::now(), Some(ttoken)));
            // end of sending
            return;
        }

        // handle FIN sending
        if let TransType::FIN = ttype {
            self.udp_send.send(PacketCmd::FIN(sock_id.clone(), seq_num)).unwrap();
            sock_content.print_log(&sock_id, "F");
            assert!(seq_num == sock_content.send_base || seq_num == sock_content.recv_next);
            if retrans_flag {
                println!("Retransmiting FIN !!!");
                sock_content.print_log(&sock_id, "!");
                // do not keep resend FIN, you may not get reply
                // all done, connection is closed.
                self.ports_alloc[sock_id.local_port as usize] = false;
                self.socket_map.remove(&sock_id);
                return;
            }
            // set up new timer if not retrans
            self.timer_send.send(TimerCmd::New(
                Instant::now() + sock_content.rtt * Self::TIMEOUT_MULTI, // time limit
                TaskMsg::SendNow(sock_id.clone(), ttype.clone(), seq_num, len, true) // timeour callback
            )).unwrap();
            // record timer token
            let ttoken = self.ttoken_recv.recv().unwrap();
            sock_content.packet_times.insert(seq_num, (Instant::now(), Some(ttoken)));
            // end of sending
            return;
        }


        // Check all the windows
        // how many bytes we can still send?
        let cong_lim = sock_content.send_cong_ctrl as isize + sock_content.send_base as isize - seq_num as isize;
        let win_left = cmp::min(sock_content.send_flow_ctl as isize, cong_lim);
        println!("cong_window = {}, win_left = {}, len_to_send = {}", sock_content.send_cong_ctrl, win_left, len);
        
        // if window is not large enough
        if win_left < Self::MSS as isize {
            // how many times we have been limited by the window
            sock_content.win_counter =  (sock_content.win_counter + 1) % 50;
            // after ten times limit, try to reach out.
            // this is useful when one ACK is dropped. And each side is waiting for the other side to move first.
            if sock_content.win_counter != 0 {

                // The send base
                if retrans_flag || sock_content.send_base == seq_num {
                    // update timer to transmit later
                    println!("Retrans timer set up.");
                    self.timer_send.send(TimerCmd::New(
                        Instant::now() + sock_content.rtt * Self::TIMEOUT_MULTI, 
                        TaskMsg::SendNow(sock_id.clone(), ttype.clone(), seq_num, len, retrans_flag)
                    )).unwrap();

                    // this packet_times is used for rtt estimation and timer cancelation, if not transmited yet.
                    // we do not need update it.
                    let ttoken = self.ttoken_recv.recv().unwrap();
                    let time_entry = sock_content.packet_times.remove(&seq_num);
                    // if it has been transmitted before
                    if time_entry.is_some() {
                        sock_content.packet_times.insert(seq_num, (time_entry.unwrap().0, Some(ttoken)));
                    }
                }

                return;
            }
        }

        // update sending length
        // it needs to be limited by the window, but it can not be zeros !
        // otherwise, the empty data packet will repeat itself. scheduled continuously
        // if this packet has eacaped from above filter.
        if win_left > Self::MSS as isize {
            len = cmp::min(win_left as u32, len);
        }
        if len == 0{
            println!("transmission canceled due to zero len");
            return;
        }


        // to send data
        // send packets until done
        loop {
            println!("Looping ... ");
            let to_send = cmp::min(packet::TransportPacket::MAX_PAYLOAD_SIZE, len as usize);
            let data = sock_content.send_buf.as_mut().unwrap().make_contiguous();
            let start_index = (seq_num - sock_content.send_base) as usize;
            let data = Vec::from(&data[start_index..start_index+to_send]);

            
            self.udp_send.send(PacketCmd::DATA(sock_id.clone(), seq_num, data)).unwrap();
            sock_content.print_log(&sock_id, ".");
            if retrans_flag {
                println!("Retransmitting DATA packet !");
                sock_content.print_log(&sock_id, "!");
            }

            // record packet trans time
            // Update timer if we are at the send base
            if seq_num == sock_content.send_base {
                // set up timer
                self.timer_send.send(TimerCmd::New(
                    Instant::now() + sock_content.rtt * Self::TIMEOUT_MULTI, 
                    TaskMsg::SendNow(sock_id.clone(), ttype.clone(), seq_num, len, true)
                )).unwrap();
                let ttoken = self.ttoken_recv.recv().unwrap();
                sock_content.packet_times.insert(seq_num, (Instant::now(), Some(ttoken)));
            } else {
                sock_content.packet_times.insert(seq_num, (Instant::now(), None));
            }

            if to_send >= len as usize {break;}
            else {
                len -= to_send as u32;
                seq_num += to_send as u32;
            }
        }

    }

    /// handler function when a packet is received.
    // when ACK is recevied , the data in buf will be sent. The window slides.
    fn handle_receive (&mut self, packet: TransportPacket) {
        let sock_id = packet.get_sock_id();
        println!("OnReceive(): Got a new packet! from {}:{}", sock_id.remote_addr, sock_id.remote_port);
        let sock_content_ref: &mut SocketContents = 
            // try normal socket first
            if let Some(content) = self.socket_map.get_mut(&sock_id) {
                content
            } else {
                // try server socket
                let mut server_sock_id =  sock_id.clone();
                server_sock_id.remote_addr = Ipv4Addr::new(0, 0, 0, 0);
                server_sock_id.remote_port = 0;
                let try_find = self.socket_map.get_mut(&server_sock_id);
                if try_find.is_some() {
                    try_find.unwrap()
                } else {
                    // if a SYN, we should send back FIN to indicate that connection is refused.
                    if let TransType::SYN = packet.get_type() {
                        self.udp_send.send(PacketCmd::FIN(sock_id.clone(), packet.get_seq_num())).unwrap();
                    }
                    // if a FIN, we should do nothing
                    return;
                }
            };

        // check seq_num, discard the packet if seq_num is out of range
        if sock_content_ref.send_buf.is_some() {
            if packet.get_seq_num() < sock_content_ref.send_base {
                // useless ACK
                println!("OnReceive(): Discard a new packet for send buf!");
                // got an acknowledgement packet that 'does not' advances the acknowledgement field
                sock_content_ref.print_log(&sock_id, "?");
                return;
            }
        } else if sock_content_ref.recv_buf.is_some() {
            if packet.get_seq_num() < sock_content_ref.recv_next {
                // useless retransmission, we had a ACK lost.
                println!("OnReceive(): Discard a new packet for recv buf!");
                sock_content_ref.print_log(&sock_id, "!");
                // need to resend the ACK
                // send ACK for DATA packet !
                let wind_left = sock_content_ref.recv_wind + sock_content_ref.recv_base - sock_content_ref.recv_next;
                self.udp_send.send(PacketCmd::ACK(sock_id, wind_left, sock_content_ref.recv_next)).unwrap();
                return;
            }
        }

        match packet.get_type() {
            TransType::SYN => {
                println!("OnReceive(): Got a SYN packet!");
                sock_content_ref.print_log(&sock_id, "s");
                // what if we got a retransmited SYN? -> just update the socket list?
                // 1. if SYN is lost, the client will retransmit, no processing required,
                // 2. if ACK is lost, the client will retransmit. The packet will match the new connection socket,
                //    and since its state is not listen, nothing happens.
                if let SocketState::LISTEN = sock_content_ref.state{
                    // Send ACK for SYN
                    self.udp_send.send(PacketCmd::ACK(sock_id.clone(), Self::BUFFER_CAP as u32, packet.get_seq_num()+1)).unwrap();
                    
                    // init a new socket for the incoming connection
                    // 1. new socket content
                    let (ret_send, ret_recv) =  mpsc::channel::<TaskRet>();
                    let mut new_sock_content = SocketContents::new(ret_send);
                    new_sock_content.state = SocketState::ESTABLISHED;
                    new_sock_content.recv_buf = Some(VecDeque::with_capacity(Self::BUFFER_CAP));
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
                println!("OnReceive(): Got a ACK packet! seq {}", packet.get_seq_num());
                // update flow control window
                sock_content_ref.send_flow_ctl = packet.get_wind() as usize;
                println!("New flow control window = {}", sock_content_ref.send_flow_ctl);
                if sock_content_ref.send_buf.is_some() && packet.get_seq_num() == sock_content_ref.send_base {
                    // useless ack for client socket
                    // got an acknowledgement packet that 'does not' advances the acknowledgement field
                    println!("A duplicate ACK !");
                    sock_content_ref.print_log(&sock_id, "?");
                    // if new dup ack packet
                    if packet.get_seq_num() != sock_content_ref.dup_ack_record {
                        sock_content_ref.dup_ack_num = 1;
                        sock_content_ref.dup_ack_record = packet.get_seq_num();
                    } else {
                        sock_content_ref.dup_ack_num += 1;
                        // if three dup ACKs
                        if sock_content_ref.dup_ack_num == 3 {
                            // update congestion control window
                            sock_content_ref.send_cong_ctrl /= 2;
                            // do a fast retransmission ?
                            // Do not, we are not implementing Reno ...
                            // self.task_send.send(TaskMsg::SendNow(sock_id, TransType::DATA, sock_content_ref.send_base, Self::MSS as u32, true)).unwrap();
                        }
                    }
                    return;
                }
                // Congestion Control ==== > additive increase
                // increases window by 1 per round-trip time
                sock_content_ref.send_cong_ctrl += Self::MSS * Self::MSS / sock_content_ref.send_cong_ctrl ;
                // got an acknowledgement packet that advances the acknowledgement field
                sock_content_ref.print_log(&sock_id, ":");
                match sock_content_ref.state {
                    SocketState::SYN_SENT => {
                        // check seq_num
                        assert_eq!(packet.get_seq_num(), sock_content_ref.send_base + 1);
                        // switch established state
                        sock_content_ref.state = SocketState::ESTABLISHED;
                        // move the window
                        sock_content_ref.send_base = packet.get_seq_num();
                        assert!(sock_content_ref.send_base <= sock_content_ref.send_next);
                        println!("OnReceive(): New connection established!");

                        let send_seq_num = packet.get_seq_num() - 1;
                        // update rtt and remove retrans timer
                        if let Some(time_entry) = sock_content_ref.packet_times.remove(&send_seq_num) {
                            if time_entry.1.is_some() {
                                // if we set up a timer earlier, cancel it
                                self.timer_send.send(TimerCmd::Cancel(time_entry.1.unwrap())).unwrap();
                            }
                            let now = Instant::now();
                            sock_content_ref.rtt =  sock_content_ref.rtt.mul_f64(Self::RTT_RATIO) + (now - time_entry.0).mul_f64(1.0 - Self::RTT_RATIO);
                            if sock_content_ref.rtt < Duration::from_millis(Self::RTT_LOW_BOUND) {
                                sock_content_ref.rtt = Duration::from_millis(Self::RTT_LOW_BOUND);
                            }
                        }
                    }
                    SocketState::ESTABLISHED => {
                        // check that we have a send buf
                        assert!(sock_content_ref.send_buf.is_some());

                        // delete data in buf
                        let to_drain = packet.get_seq_num() - sock_content_ref.send_base;
                        sock_content_ref.send_buf.as_mut().unwrap().drain(0..to_drain as usize);
                        // move the window
                        let old_send_base = sock_content_ref.send_base;
                        sock_content_ref.send_base = packet.get_seq_num();
                        assert_eq!(sock_content_ref.send_buf.as_ref().unwrap().len() as u32, sock_content_ref.send_next - sock_content_ref.send_base);
                        assert!(sock_content_ref.send_base <= sock_content_ref.send_next);

                        // update rtt and cancel timer if needed
                        if let Some(time_entry) = sock_content_ref.packet_times.remove(&old_send_base) {
                            if time_entry.1.is_some() {
                                // if we set up a timer earlier, cancel it
                                self.timer_send.send(TimerCmd::Cancel(time_entry.1.unwrap())).unwrap();
                            }
                            let now = Instant::now();
                            sock_content_ref.rtt =  sock_content_ref.rtt.mul_f64(Self::RTT_RATIO) + (now - time_entry.0).mul_f64(1.0 - Self::RTT_RATIO);
                            if sock_content_ref.rtt < Duration::from_millis(Self::RTT_LOW_BOUND) {
                                sock_content_ref.rtt = Duration::from_millis(Self::RTT_LOW_BOUND);
                            }
                        }
                        // update retrans timer, if we have data in buf left unacked
                        if sock_content_ref.send_base < sock_content_ref.send_next {
                            // it may or may not has been transmitted
                            let new_time_entry = sock_content_ref.packet_times.remove(&sock_content_ref.send_base);
                            if new_time_entry.is_some() {
                                assert!(new_time_entry.unwrap().1.is_none());
                                // update timer
                                self.timer_send.send(TimerCmd::New(
                                    new_time_entry.unwrap().0 + sock_content_ref.rtt * Self::TIMEOUT_MULTI, 
                                    TaskMsg::SendNow(
                                        sock_id.clone(), TransType::DATA, sock_content_ref.send_base, 
                                        sock_content_ref.send_next - sock_content_ref.send_base,
                                        true
                                    )
                                )).unwrap();
                                // put back the packet records!
                                let ttoken = self.ttoken_recv.recv().unwrap();
                                sock_content_ref.packet_times.insert(sock_content_ref.send_base, (new_time_entry.unwrap().0, Some(ttoken)));
                            }
                            // the new send base has not been sent before
                            else {
                                self.timer_send.send(TimerCmd::New(
                                    Instant::now() + sock_content_ref.rtt * Self::TIMEOUT_MULTI, 
                                    TaskMsg::SendNow(sock_id.clone(), TransType::DATA, 
                                                     sock_content_ref.send_base, sock_content_ref.send_next - sock_content_ref.send_base, false)
                                )).unwrap();
                            }

                        }
                    }
                    SocketState::FIN_WAIT => {
                        // check that we have a send buf
                        assert!(sock_content_ref.send_buf.is_some());

                        // check seq_num
                        if packet.get_seq_num() == sock_content_ref.send_next {
                            sock_content_ref.send_base = packet.get_seq_num();
                            // cancel timer
                            if let Some(time_entry) = sock_content_ref.packet_times.remove(&sock_content_ref.send_base) {
                                if time_entry.1.is_some() {
                                    // if we set up a timer earlier, cancel it
                                    self.timer_send.send(TimerCmd::Cancel(time_entry.1.unwrap())).unwrap();
                                }
                            }
                            // we are good to close now.
                            // send FIN, get into FIN_SENT state.
                            self.task_send.send(TaskMsg::SendNow(sock_id, TransType::FIN, sock_content_ref.send_next, 0, false)).unwrap();
                            sock_content_ref.state = SocketState::FIN_SENT;
                        } else {
                            // delete data in buf
                            let to_drain = packet.get_seq_num() - sock_content_ref.send_base;
                            sock_content_ref.send_buf.as_mut().unwrap().drain(0..to_drain as usize);
                            // move the window
                            let old_send_base = sock_content_ref.send_base;
                            sock_content_ref.send_base = packet.get_seq_num();
                            assert_eq!(sock_content_ref.send_buf.as_ref().unwrap().len() as u32, sock_content_ref.send_next - sock_content_ref.send_base);
                            assert!(sock_content_ref.send_base <= sock_content_ref.send_next);

                            // update rtt
                            if let Some(time_entry) = sock_content_ref.packet_times.remove(&old_send_base) {
                                if time_entry.1.is_some() {
                                    // if we set up a timer earlier, cancel it
                                    self.timer_send.send(TimerCmd::Cancel(time_entry.1.unwrap())).unwrap();
                                }
                                let now = Instant::now();
                                sock_content_ref.rtt =  sock_content_ref.rtt.mul_f64(Self::RTT_RATIO) + (now - time_entry.0).mul_f64(1.0 - Self::RTT_RATIO);
                                if sock_content_ref.rtt < Duration::from_millis(Self::RTT_LOW_BOUND) {
                                    sock_content_ref.rtt = Duration::from_millis(Self::RTT_LOW_BOUND);
                                }
                            }
                            // update retrans timer, if we have data in buf left unacked
                            if sock_content_ref.send_base < sock_content_ref.send_next {
                                // it may or may not has been transmitted
                                let new_time_entry = sock_content_ref.packet_times.remove(&sock_content_ref.send_base);
                                if new_time_entry.is_some() {
                                    assert!(new_time_entry.unwrap().1.is_none());
                                    // update timer
                                    self.timer_send.send(TimerCmd::New(
                                        new_time_entry.unwrap().0 + sock_content_ref.rtt * Self::TIMEOUT_MULTI, 
                                        TaskMsg::SendNow(
                                            sock_id.clone(), TransType::DATA, sock_content_ref.send_base, 
                                            sock_content_ref.send_next - sock_content_ref.send_base,
                                            true
                                        )
                                    )).unwrap();
                                    // put back the packet records!
                                    let ttoken = self.ttoken_recv.recv().unwrap();
                                    sock_content_ref.packet_times.insert(sock_content_ref.send_base, (new_time_entry.unwrap().0, Some(ttoken)));
                                }
                                // the new send base has not been sent before
                                else {
                                    self.timer_send.send(TimerCmd::New(
                                        Instant::now() + sock_content_ref.rtt * Self::TIMEOUT_MULTI, 
                                        TaskMsg::SendNow(sock_id.clone(), TransType::DATA, 
                                                        sock_content_ref.send_base, sock_content_ref.send_next - sock_content_ref.send_base, false)
                                    )).unwrap();
                                }
                            }
                        }
                    }

                    SocketState::FIN_SENT => {
                        println!("ACK received, Socket is destroied.");
                        // cancel retrans timer
                        if let Some(time_entry) = sock_content_ref.packet_times.remove(&(packet.get_seq_num() -1 )) {
                            if time_entry.1.is_some() {
                                println!("FIN timer canceled!");
                                // if we set up a timer earlier, cancel it
                                self.timer_send.send(TimerCmd::Cancel(time_entry.1.unwrap())).unwrap();
                            }
                        }
                        // all done, connection is closed.
                        self.ports_alloc[sock_id.local_port as usize] = false;
                        self.socket_map.remove(&sock_id);

                    }
                    _ => {
                        println!("This state is not handled yet.")
                    }
                }
            }
            TransType::DATA => {
                println!("OnReceive(): Got a DATA packet!");
                sock_content_ref.print_log(&sock_id, ".");
                if let SocketState::ESTABLISHED = sock_content_ref.state {
                    // First, check the seq_num to see whether a packet is lost.
                    if packet.get_seq_num() == sock_content_ref.recv_next {
                        // copy as many as we can, discard the rest.
                        let buf_left =  sock_content_ref.recv_wind - (sock_content_ref.recv_next - sock_content_ref.recv_base);
                        let to_recv = cmp::min(buf_left, packet.get_payload_len());
                        
                        // copy to recv buf
                        if packet.get_payload().is_some() {
                            sock_content_ref.recv_buf.as_mut().unwrap().extend(packet.get_payload().unwrap()[..to_recv as usize].iter());
                        }
                        sock_content_ref.recv_next += to_recv;

                        // make sure the buf is behaving correctly.
                        assert!(sock_content_ref.recv_next - sock_content_ref.recv_base <= sock_content_ref.recv_wind);
                        assert!(sock_content_ref.recv_next - sock_content_ref.recv_base == sock_content_ref.recv_buf.as_ref().unwrap().len() as u32);

                        // send ACK for DATA packet !
                        let wind_left = sock_content_ref.recv_wind + sock_content_ref.recv_base - sock_content_ref.recv_next;
                        self.udp_send.send(PacketCmd::ACK(sock_id, wind_left, sock_content_ref.recv_next)).unwrap();
                    }
                    // if seq_num is not continuous
                    else {
                        // we do not take that data ...
                        // send ACK for DATA packet !
                        let wind_left = sock_content_ref.recv_wind + sock_content_ref.recv_base - sock_content_ref.recv_next;
                        self.udp_send.send(PacketCmd::ACK(sock_id, wind_left, sock_content_ref.recv_next)).unwrap();
                    }

                } else if let SocketState::FIN_SENT = sock_content_ref.state {
                    // we do nothing, refuse to provide any service as a server.
                    println!("No ACK since I am in FIN_SENT.");
                } else {
                    panic!("Wrong state, should not receive data packet!");
                }
            }
            TransType::FIN => {
                println!("OnReceive(): Got a FIN packet!");
                sock_content_ref.print_log(&sock_id, "f");
                if let SocketState::ESTABLISHED = sock_content_ref.state {
                    // if it is a client socket
                    if sock_content_ref.send_buf.is_some() {
                        // close immediately, the server just hang up.
                        sock_content_ref.state = SocketState::CLOSED;
                        sock_content_ref.send_buf = None;
                        sock_content_ref.send_base = 0;
                        sock_content_ref.send_next = 0;

                        // send ACK
                        self.udp_send.send(PacketCmd::ACK(sock_id, 0, packet.get_seq_num() + 1)).unwrap();
                    }
                    else if sock_content_ref.recv_buf.is_some() {
                        // the connection is closed, but user can still read buf.
                        sock_content_ref.state = SocketState::SHUTDOWN;
                        // send ACK
                        self.udp_send.send(PacketCmd::ACK(sock_id, 0, packet.get_seq_num() + 1)).unwrap();
                    }

                } else {
                    panic!("Wrong state, should not receive FIN packet!");
                }
            }
        }
    }



}