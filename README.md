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

**Does it runs?**

For local_test and window_test, the program runs with no error even when packet loss rate equals 5%. 

When tested on two real machines, one runs the client program and one runs the server program. I used two distant cloud machines, which have a 100+ ms delay. Two concurrent connections are created and 40KB data are sent for each connection. The test completes successfully with a simulated packet loss rate less than 3%. Some error will happen when the packet loss is higher than 3%.

## Part 1
### Design of Implementation

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

## Part 2 : Flow Control and Congestion Control
### Disscussions of design
1. Diss2: Your transport protocol implementation picks the size of a buffer for received data that is used as part of flow control. How large should this buffer be, and why?

The buffer should be at least larger than the max size of one packet. So the packet delivery can move forword. Ideally, the larger the buffer is, the more efficient the transport would be. Because a large buffer can endure bursts of received packets without telling the sender to slow down.

## Part 3 : Design Extensions
