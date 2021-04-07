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

**How well it runs? **

For local_test and window_test, the program runs with no error even when packet loss rate equals 5%. 

When tested on two real machines, one runs the client program and one runs the server program. I used two distant cloud machines, which have a 100+ ms delay. Two concurrent connections are created and 40KB data are sent for each connection. The test completes successfully with a simulated packet loss rate less than 3%. Some error will happen when the packet loss is higher than 3%.

## Part 1