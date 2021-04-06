use std::{collections::VecDeque, convert::TryInto, net::Ipv4Addr, u32, usize};

use super::socket::SocketID;

/// transport packet, following fishnet format

/**
 * <pre>   
 * This conveys the header for reliable message transfer.
 * This is carried in the payload of a Packet, and in turn the data being
 * transferred is carried in the payload of the Transport packet.
 * </pre>   
 */

#[repr(u8)]
#[derive(Clone, Debug)]
pub enum TransType {
     SYN = 0,
     ACK,
     FIN,
     DATA,
 }

 impl Default for TransType {
     fn default() -> Self {
         TransType::DATA
     }
 }

 impl TransType {
    fn new(t: u8) -> Self {
        match t {
            0 => Self::SYN,
            1 => Self::ACK,
            2 => Self::FIN,
            _ => Self::DATA,
        }
    }
 }

 #[derive(Clone)]
pub struct TransportPacket {
    src_port: u8,
    dest_port: u8,
    t_type: TransType,
    window: u32,
    seq_num: u32,
    total_len: u8,
    payload: Option<Vec<u8>>,
    // To keep recording only, do not inclue them in the packet
    src_addr: Ipv4Addr,
    dest_addr: Ipv4Addr,
}

impl TransportPacket {
    const HEADER_SIZE: usize = 12;
    const MAX_PACKET_SIZE: usize = 212; // we use only one byte for packet length (fishnet only has 128 bytes max len)
    pub const MAX_PAYLOAD_SIZE: usize = TransportPacket::MAX_PACKET_SIZE - TransportPacket::HEADER_SIZE;

    /// for sending packet, init with specific values
    pub fn new (src_port: u8, dest_port: u8, t_type: TransType, window: u32, seq_num: u32, mut payload: Option<Vec<u8>>) -> Self {
        let mut total_len: u8 = Self::HEADER_SIZE as u8;
        if payload.is_some() {
            let load = payload.unwrap();
            assert!(load.len() <= Self::MAX_PAYLOAD_SIZE);
            total_len = (Self::HEADER_SIZE + load.len()) as u8;
            payload = Some(load);
        }

        TransportPacket{src_port, dest_port, t_type, window, seq_num, total_len, payload, 
                        src_addr: Ipv4Addr::new(127, 0, 0, 1), dest_addr: Ipv4Addr::new(127, 0, 0, 1)}
    }

    pub fn default () -> Self {
        TransportPacket{src_port: 0, dest_port: 0, t_type: TransType::ACK, window: 0, seq_num: 0, total_len: 0, payload: None, 
                        src_addr: Ipv4Addr::new(127, 0, 0, 1), dest_addr: Ipv4Addr::new(127, 0, 0, 1)}
    }

    // call default() to get an default packet to call unpack()

    /**
     * Convert the Transport packet object into a byte array for sending over the wire.
     * Format:
     *        source port = 1 byte
     *        destination port = 1 byte
     *        type = 1 byte
     *        window size = 4 bytes
     *        sequence number = 4 bytes
     *        packet length = 1 byte
     *        payload <= MAX_PAYLOAD_SIZE bytes
     * @return A byte[] for transporting over the wire. Null if failed to pack for some reason
     */
    pub fn pack (mut self) -> Vec<u8> {
        let mut packet: Vec<u8> = Vec::with_capacity(TransportPacket::MAX_PACKET_SIZE as usize);

        packet.push(self.src_port);
        packet.push(self.dest_port);
        packet.push(self.t_type as u8);
        packet.extend(&self.window.to_be_bytes());
        packet.extend(&self.seq_num.to_be_bytes());
        packet.push(self.total_len);
        if self.payload.is_some() {
            packet.append(&mut self.payload.unwrap());
        }

        return packet;
    }

    /**
     * Unpacks a byte array to create a Transport object
     * Assumes the array has been formatted using pack method in Transport
     * @param packet String representation of the transport packet
     * @return Transport object created or null if the byte[] representation was corrupted
     */
    pub fn unpack (&mut self, mut packet: VecDeque<u8>, src_addr: Ipv4Addr, dest_addr: Ipv4Addr) {
        self.src_port = packet.pop_front().unwrap();
        self.dest_port = packet.pop_front().unwrap();
        self.t_type = TransType::new(packet.pop_front().unwrap());
        self.window = u32::from_be_bytes(packet.drain(0..4).collect::<Vec<u8>>().try_into().unwrap());
        self.seq_num = u32::from_be_bytes(packet.drain(0..4).collect::<Vec<u8>>().try_into().unwrap());
        self.total_len = packet.pop_front().unwrap();
        assert!(packet.len() == (self.total_len as usize - Self::HEADER_SIZE), "Unpack(): Wrong payload size!");
        if packet.len() != 0 {
            self.payload = Some(packet.into());
        }
        // for manager reference only
        self.src_addr = src_addr;
        self.dest_addr = dest_addr;
    }

    // this only works for recv packet
    pub fn get_sock_id (&self) -> SocketID {
        SocketID{
            local_addr: self.dest_addr,
            local_port: self.dest_port,
            remote_addr: self.src_addr,
            remote_port: self.src_port,
        }
    }

    pub fn get_type (&self) -> TransType {
        self.t_type.clone()
    }

    pub fn get_seq_num (&self) -> u32 {
        self.seq_num.clone()
    }

    pub fn get_payload_len (&self) -> u32 {
        let load = self.payload.as_ref();
        if load.is_none() {
            0 as u32
        } else {
            load.unwrap().len() as u32
        }
    }

    pub fn get_payload (&self) -> &Vec<u8> {
        self.payload.as_ref().unwrap()
    }

    pub fn get_wind (&self) -> u32 {
        self.window.clone()
    }

}