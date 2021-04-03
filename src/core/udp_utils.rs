use core::panic;
use std::{collections::VecDeque, net::Ipv4Addr, sync::{Arc, Mutex, mpsc::{Receiver, Sender}}, thread, usize};
use std::net::{UdpSocket, IpAddr};

use super::{packet::{TransType, TransportPacket}, socket::SocketID};
use super::manager::TaskMsg;
use rand::Rng;


const UDP_IN_PORT: usize = 8848;
const UDP_OUT_PORT: usize = 8888;
// the manager sends the packet commands to the udp thread
pub enum PacketCmd {
    SYN(SocketID), // (sock_id)
    FIN(SocketID),
    ACK(SocketID, u32, u32), // (sock_id, window, seq_num)
    DATA(SocketID),
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
            PacketCmd::SYN(id) => {
                println!("UDP: SYN sending...");
                let socket = UdpSocket::bind(format!("{}:{}", id.local_addr, UDP_OUT_PORT)).unwrap();
                // no window, random seq_num, no payload
                let packet = TransportPacket::new(id.local_port, id.remote_port, 
                                                                  TransType::SYN, 0, rand::thread_rng().gen_range(0..2048), None);
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
        task_send.send(TaskMsg::OnReceive(packet)).unwrap();
    }

}
