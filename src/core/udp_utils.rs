use core::panic;
use std::{collections::VecDeque, net::Ipv4Addr, sync::mpsc::{Receiver, Sender}, thread, usize};
use std::net::UdpSocket;

use super::{packet::{TransType, TransportPacket}, socket::SocketID};
use super::manager::TaskMsg;
use rand::Rng;


const UDP_IN_PORT: usize = 8848;
const UDP_OUT_PORT: usize = 8888;
const UDP_IN_ADDR: &str = "127.0.0.1";
// the manager sends the packet commands to the udp thread
pub enum PacketCmd {
    SYN(SocketID), // (sock_id, window, seq_num, payload)
    FIN,
    ACK,
    DATA,
}

// == entry point of the udp loop thread ==
// actually we need two loops, one for sending, one for recv
pub fn start_loops (cmd_recv: Receiver<PacketCmd>, task_send: Sender<TaskMsg>) {
    thread::spawn(move || {
        in_loop(task_send);
    });

    thread::spawn(move || {
        out_loop(cmd_recv);
    });

}

// sending out packets
// the manager can send cmds to udp loop
fn out_loop (cmd_recv: Receiver<PacketCmd>) {
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
            }
            _ => {}
        }
    }
}

// recv incoming packets
// the udp dispatcher can call manager to handler received packets
fn in_loop (task_send: Sender<TaskMsg>) {
    let socket = UdpSocket::bind(format!("{}:{}", UDP_IN_ADDR, UDP_IN_PORT)).unwrap();
    loop {
        let mut in_buf = [0; 2048];
        let (_amt, src) = socket.recv_from(&mut in_buf).unwrap();
        println!("Here!!!");
        let mut packet = TransportPacket::default();
        let tmp = VecDeque::from(Vec::from(in_buf));
        println!("This is inefficient. Len = {}", tmp.len());
        packet.unpack(tmp);
        task_send.send(TaskMsg::OnReceive(packet)).unwrap();
    }

}
