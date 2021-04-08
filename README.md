# CS434/534 Programming Assignment 2: Network Transport
This serves as the report for this assignment.

**File Descriptions**
```
|-- src
|    |-- node.rs : The executable program. Run this to start a transport node.
|    |-- lib.rs : The lib for all modules needed for my transport protocol implementation.
|    |-- core : The folder holds all modules of the lib.
|         |-- socket.rs : The socket API used by node.rs . All basic methods are non-blocking.
|         |-- manager.rs : The manager struct to govern all the sockets. It communicates with the timer
|         |                and udp sender and receiver.
|         |-- packet.rs : The transport packet format and some helper functions.
|         |-- timer.rs : The timer struct for setting up timeout.
|         |-- udp_utils.rs : The underlaying udp sender and receiver.
|-- target
|    |-- debug : The compiled program. This one only wroks for macos x86_64.
|    |-- logs : The logs for each socket, recording packets sent or recv.
```

## To Run
There are threes exec modes for `./target/debug/node_bin`
```
// this will compile the code (if you installed rust)
cargo build

// run this to see all the arguments available
./target/debug/node_bin --help

// basic functionality test running on 127.0.0.1
./target/debug/node_bin --ex local_test

// A test for flow control. A fast sender and a slow receiver. Also running with localhost.
./target/debug/node_bin --ex window_test

// Transfer 40KB testing data to the server side. The server program needs to be running first on another machine.
./target/debug/node_bin --ex transfer --da 'dest ip addr' --la 'local ip addr'

// set up a server socket listener, which with continously accept new connections.
./target/debug/node_bin --ex server --la 'local ip addr'

```
To simulate some packet loss, go to `./src/core/udp_utils.rs` and change `SIM_LOSS_RATE`. This defines the probability of a packet loss.

**Does it wroks?**

For local_test and window_test, the program runs with no error even when packet loss rate equals 5%. 

When tested on two real machines, one runs the client program and one runs the server program. I used two distant cloud machines, which have a 100+ ms delay. Two concurrent connections are created and 40KB data are sent for each connection. The test completes successfully if no simulated packet loss.

## Part 1
### High Level Design

From the perspective of multi-thread programming, I have four threads running when I start the node. All the threads send messages to each other through channels/pipes.

First, we have *the user application thread*, i.e. the main thread, created by the `node.rs`. Inside this thread, it will bring up *the manager thread* in `manager.rs`, which will provide transport service. The `socket.rs` is only an API and does not do any work. When the user thread calls the socket API, it only sends a task message to the manager. The manager holds a task queue, which is the most important component of it. All it does during execution is to process next task in the queue.

Second, when the manager thread starts, it will also bring up *the timer thread* and *the udp-utils thread*. The manager can tell the timer thread to set up or cancel timeout timer. The timer will put a task into the queue when timeout happens. The udp thread only does two things. It unpack packets and hand them over to mananger thread when new datagrams arrived using UDP. It also packs packets in its queue and send them to remote host using UDP.

### Disscussions of design
1. Diss1a: Your transport protocol implementation picks an initial sequence number when establishing a new connection. This might be 1, or it could be a random value. Which is better, and why?

I chose a random number as the initial sequence number. It seems better than 1 because it makes sure that the ack from the server side is fresh.

2. Diss1b: Our connection setup protocol is vulnerable to the following attack. The attacker sends a large number of connection request (SYN) packets to a particular node, but never sends any data. (This is called a SYN flood.) What happens to your implementation if it were attacked in this way? How might you have designed the initial handshake protocol (or the protocol implementation) differently to be more robust to this attack?

If all the SYNs are from the same port, one connection will be established and that is all. All the following SYN will be ignored. If SYN are from different ports, only limited number of new connections will be established, because the number of backlog is limited. To prevent the attacker from using up the backlog, we can limit the number of SYN accepted from the same ip addr at a certain period of time, assuming that the attacker can not easily get a large number of ip addresses. 

1. Diss1c: What happens in your implementation when a sender transfers data but never closes a connection? (This is called a FIN attack.) How might we design the protocol differently to better handle this case?

Given my current design, this will leave a connection open infinitely. This is undesriable. We could potentially use time out to regulate the max duration of an idle connection.

### Implementation Details
1. `The socket manager` struct mantains a task queue to hold all the tasks to handle. I reused the inter-thread communication channel/pipe as the queue. Since the message in the channel is also FIFO. Rust enum can hold various types of values, so I use this to hold all the task arguments for socket APIs and other tasks.
```
// Task Message from socket api and timer sent to socket manager
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
```

2. `The socket manager` also matains a hashmap containing pairs of `SocketID` and `SocketContens`. `SocketID` contains a four-element tuple, (local_addr, local_port, remote_addr, remote_port). The `SocketContents` holds all the buffers and sliding windows. As described above, the socket API does not have any info and does not do any work. And all APIs are non-blocking.

3. The sending and recving buffers are implemented suing `VecDeque<u8>`, which is a ring buffer of bytes. Several 32-bit values are used to define the sliding window. For sending window, send_base, send_next, send_wind are used. For recv window, recv_base, recv_next, recv_wind are used.
```
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
```

4. I use a special timer thread to schedule the retransmission task. The socket manager can send command to the timer thread to set up or tear down a timeout timer. The socket manager will also provide a callback, which is a task for the manager. When the timer is triggered, the timer thread will put the callback into the manager's task queue to perform retransmission.
```
type TimeoutCallback = TaskMsg;
pub enum TimerCmd {
    New(time::Instant, TimeoutCallback),
    Cancel(TimerToken),

}
struct TimerEntry {
    time_lim: time::Instant,
    callback: TimeoutCallback,
}
```

5. For each socket, there are seven states.
```
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
```
 
### Test Outputs
Here shows the test output of `lcoal_test`. In this test, one client and one server socket are created. They communate over UDP on localhost. Frist, the client sends 100 packets to the server. The sever will read the data right after one packet is sent. Then, the client sends 20 packets to fill the server buffer. And the sever reads 20 packets. This uses the buffer but does not involve flow control. The packt loss rate is 5%
**Client Outputs**
```
S:.:.:..!:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:..!:.:.:.:.:.:.:.:.:..!:.:.:..!:.:.:.:.:..!:.:.:..!:.:.:.:.:..!:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:..!:.:.:.:..!:.:.:.:.:.:..!:.:....::..::.:.:..::.::...::..:.:.::.:.:::?F:
```
**Server Outputs**
```
............................................................!...!.....!....................................................f
```
My notion is slight different from the assignment document. In my output, `.` is a normal data packet, `.!` is a retransmited data packet.

As the outputs show, all the data packet will get an ACK, and retransmit if it is lost. The received data is examined at the server side.

## Part 2 : Flow Control and Congestion Control
### Disscussions of design
1. Diss2: Your transport protocol implementation picks the size of a buffer for received data that is used as part of flow control. How large should this buffer be, and why?

The buffer should be at least larger than the max size of one packet. So the packet delivery can move forword. Ideally, the larger the buffer is, the more efficient the transport would be. Because a large buffer can endure bursts of received packets without telling the sender to slow down.

### Details fo design
Several varibales inside the socket content are used for flow and congestion control. 
```
/// flow control, recv window left
send_flow_ctl: usize,
/// conjestion control, num of bytes
send_cong_ctrl: usize,
dup_ack_record: u32,
dup_ack_num: u32,
```
`send_flow_ctl` will be updated when new ACK arrives. And `send_cong_ctrl` will be updated accroding to AIMD. When three duplicate ACKs are recorded, it will be cut to half. And it will additively increase when ACK arrives normally.

There are some cases to consider when we use flow control. If the server's buffer is full and the sender stops sending packets, how to restart to transportation? To solve this issue, the server socket will send a reduntant ACK carrying new window size when the full recv buffer has been read and gets room again. But this updated ACK may also get lost. So I also allow the client to ignore the flow control window and send a data packet to get a ACK from the server. This will only happens when client is limited by the window after many timeouts.

### Test Outputs
Here shows the results of `./target/debug/node_bin --ex window_test`. In this test, the client is sending packets in a much faster speed than the server read the data. So the flow control will work. And considering simulated packet loss, congestion control will also work.
**Client Outputs**
```
S:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:.:?.?.?..:?.:?.:.??.!:?....:??.??...!..:???.??..:?.?.??.!:.?.??.!:.!?.??..!.:??..!?.?.!:.?....:???.?.?.?.??.:...?.:.?.??....!.:???..?.!?.?.!:.?.?.?.??...!.........!..!:???????????.?.?.?.?.?.!.!:??.?..?.??.....!:?????f
```
**Server Outputs**
```
..............................!!!..!!!!..!.........!!.....!!!............!!!!.........!!!!!!!!!!!.....!!.....!!!!F:
```

## Part 3 : Design Extensions
Your design can be extended in multiple directions to integrate modern features. Please pick 3 of 4 the following directions.

1. Multipath TCP API and protocol: Your current API is the basic socket API. In class, we discussed multi-path TCP. How may you extend your socket to allow multi-path API? Please (1) specify the issues of existing API to support multipath TCP, (2) list the new API, clearly marking modifications and/or additions to the API, (3) give an example of client and server programs using your multi-path TCP API, and (4) briefly describe how your implementation (protocol format, server, client) will be changed to support multipath TCP. For this design exercise, you can assume bi-directional transport.

    (1) 


2. Secure Transport API and protocol: One direction of modern transport design is the integration (e.g., in QUIC) of basic transport (TCP) and security (e.g., TLS). Please provide a basic, high-level API and protocol design which integrates basic transport and TLS security.


3. Congestion Control: Please describe the modification of your code (as concrete as you can) to implement TCP Cubic congestion control. Please describe briefly how your code can be extended to use Google's BBR v1 congestion control.

4. Delivery Flexibilities: Some major networks (e.g., Amazon Scalable Reliable Datagram) propose that the network does not enforce in-order delivery. Please describe how you may design a flexible API and protocol so that the transport can provide flexibilities such as delivery order (segments can be delivered to applications not in order) and reliability semantics/flexibilities (e.g., some packets do not need reliability, and one can use FEC to correct errors instead of using retransmissions).