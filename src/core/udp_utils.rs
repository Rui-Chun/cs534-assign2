use core::panic;
use std::{collections::VecDeque, net::Ipv4Addr, sync::{Arc, Mutex, mpsc::{Receiver, Sender}}, thread, usize};
use std::net::{UdpSocket, IpAddr};
use rand::Rng;

use super::{packet::{TransType, TransportPacket}, socket::SocketID};
use super::manager::TaskMsg;

const UDP_IN_PORT: usize = 8848;
const UDP_OUT_PORT: usize = 8888;
const SIM_LOSS_RATE: f64 = 0.05;
// the manager sends the packet commands to the udp thread
#[derive(Clone)]
pub enum PacketCmd {
    SYN(SocketID, u32), // (sock_id)
    /// (sock_id, seq_num)
    FIN(SocketID, u32),
    /// (sock_id, window, seq_num)
    ACK(SocketID, u32, u32),
    /// (sock_id, seq_num, payload)
    DATA(SocketID, u32, Vec<u8>),
}

// == entry point of the udp loop thread ==
// actually we need two loops, one for sending, one for recv
pub fn start_loops (cmd_recv: Receiver<PacketCmd>, task_send: Sender<TaskMsg>) {
    let udp_in_addr: Arc<Mutex<Ipv4Addr>> = Arc::new(Mutex::new(Ipv4Addr::new(127, 0,0, 1)));
    let udp_in_addr_c = udp_in_addr.clone();

    thread::spawn(move || {
        in_loop(task_send, udp_in_addr_c);
    });

    thread::spawn(move || {
        out_loop(cmd_recv, udp_in_addr);
    });

}

// sending out packets
// the manager can send cmds to udp loop
fn out_loop (cmd_recv: Receiver<PacketCmd>, udp_in_addr: Arc<Mutex<Ipv4Addr>>) {
    // parse the commands
    for cmd in cmd_recv {
        match cmd {
            PacketCmd::SYN(id, seq_num) => {
                println!("UDP: SYN sending...");
                let socket = UdpSocket::bind(format!("{}:{}", id.local_addr, UDP_OUT_PORT)).unwrap();
                // no window, random seq_num, no payload
                let packet = TransportPacket::new(id.local_port, id.remote_port, 
                                                                  TransType::SYN, 0, seq_num, None);
                let out_buf = packet.pack();
                let amt = socket.send_to(&out_buf, format!("{}:{}", id.remote_addr, UDP_IN_PORT)).unwrap();
                if amt != out_buf.len() {
                    panic!("Can not send complete packet!");
                }
                if id.local_addr != *udp_in_addr.lock().unwrap() {
                    *udp_in_addr.lock().unwrap() = id.local_addr;
                }
            }
            PacketCmd::ACK(id, window, seq_num) => {
                println!("UDP: ACK sending...");
                let socket = UdpSocket::bind(format!("{}:{}", id.local_addr, UDP_OUT_PORT)).unwrap();
                // with window and seq_num, no payload
                let packet = TransportPacket::new(id.local_port, id.remote_port, 
                                                                  TransType::ACK, window, seq_num, None);
                let out_buf = packet.pack();
                let amt = socket.send_to(&out_buf, format!("{}:{}", id.remote_addr, UDP_IN_PORT)).unwrap();
                if amt != out_buf.len() {
                    panic!("Can not send complete packet!");
                }
            }
            PacketCmd::DATA(id, seq_num, data) => {
                println!("UDP: DATA sending...");
                let socket = UdpSocket::bind(format!("{}:{}", id.local_addr, UDP_OUT_PORT)).unwrap();
                // with window and seq_num, no payload
                let packet = TransportPacket::new(id.local_port, id.remote_port, 
                                                                  TransType::DATA, 0, seq_num, Some(data));
                let out_buf = packet.pack();
                let amt = socket.send_to(&out_buf, format!("{}:{}", id.remote_addr, UDP_IN_PORT)).unwrap();
                if amt != out_buf.len() {
                    panic!("Can not send complete packet!");
                }
            }
            PacketCmd::FIN(id, seq_num) => {
                println!("UDP: FIN sending...");
                let socket = UdpSocket::bind(format!("{}:{}", id.local_addr, UDP_OUT_PORT)).unwrap();
                // no window, seq_num, no payload
                let packet = TransportPacket::new(id.local_port, id.remote_port, 
                                                                  TransType::FIN, 0, seq_num, None);
                let out_buf = packet.pack();
                let amt = socket.send_to(&out_buf, format!("{}:{}", id.remote_addr, UDP_IN_PORT)).unwrap();
                if amt != out_buf.len() {
                    panic!("Can not send complete packet!");
                }
            }
            _ => {
                println!("unknown packet!");
            }
        }
    }
}

// recv incoming packets
// the udp dispatcher can call manager to handler received packets
fn in_loop (task_send: Sender<TaskMsg>, udp_in_addr: Arc<Mutex<Ipv4Addr>>) {
    // we only monitor one in-coming address for now...
    let mut rng = rand::thread_rng();
    let socket = UdpSocket::bind(format!("{}:{}", *udp_in_addr.lock().unwrap(), UDP_IN_PORT)).unwrap();
    loop {
        let mut in_buf = [0; 2048];
        let (amt, src) = socket.recv_from(&mut in_buf).unwrap();
        let mut packet = TransportPacket::default();
        let tmp = VecDeque::from(Vec::from(&in_buf[0..amt]));
        println!("Got packet, Len = {}", tmp.len());
        if let IpAddr::V4(src_addr) = src.ip() {
            packet.unpack(tmp, src_addr, *udp_in_addr.lock().unwrap(),);

        } else {
            panic!("can not parse ipv4 addr!");
        }

        // we will loss some packets
        if rng.gen::<f64>() > SIM_LOSS_RATE {
            task_send.send(TaskMsg::OnReceive(packet)).unwrap();
        } else {
            println!("UDP IN Loop: A packet is droped!");
        }
    }

}
