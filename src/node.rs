use clap::{App, Arg, ArgMatches};
use std::{env, sync::{Arc, Mutex, mpsc::{self, Sender, Receiver}}, thread, time};
use my_tcp::{app, core::manager::{self, TaskMsg, TaskRet}, core::socket::Socket};

fn main() {
    println!("Starting a Transport Node!");

    // Parse command line arguments
    let arg_matches = App::new("My Node Program")
        .version("0.1.0")
        .author("Ruichun Ma <ruichun.ma@yale.edu>")
        .about("A test program for my transport protocol")
        .arg(
            Arg::with_name("exec_type")
                .long("ex")
                .takes_value(true)
                .help("the type of node execution"),
        )
        // for client transfer mode
        .arg(
            Arg::with_name("dest_addr")
                .long("da")
                .takes_value(true)
                .help("address of destination node"),
        )
        .arg(
            Arg::with_name("dest_port")
                .long("dp")
                .takes_value(true)
                .help("port of destination node"),
        )
        .arg(
            Arg::with_name("local_port")
                .long("lp")
                .takes_value(true)
                .help("Port of local node"),
        )
        .arg(
            Arg::with_name("local_addr")
                .long("la")
                .takes_value(true)
                .help("Address of local node"),
        )
        .arg(
            Arg::with_name("byte_num")
                .long("bn")
                .takes_value(true)
                .help("number of bytes to transfer"),
        )
        .arg(
            Arg::with_name("interval")
                .long("int")
                .takes_value(true)
                .help("execution interval of the transfer client, default 1 second"),
        )
        .arg(
            Arg::with_name("buf_size")
                .long("bs")
                .takes_value(true)
                .help("buffer size of the transfer client, default 65536"),
        )
        // args only for server
        .arg(
            Arg::with_name("backlog")
                .long("bl")
                .takes_value(true)
                .help("buffer size of the transfer client, default 65536"),
        )
        .get_matches();

    // ===== start socket manager ====
    let mut socket_manager = manager::SocketManager::new();
    // the channel to send tasks of sockets
    let (task_sender, task_receiver) = mpsc::channel::<manager::TaskMsg>();
    // the channel to send the return value channel, channel of channel...
    let (ret_channel_send, ret_channel_recv) = mpsc::channel::<mpsc::Receiver<manager::TaskRet>>();
    let task_sender_c = task_sender.clone();
    thread::spawn(move || {
        socket_manager.start(task_sender_c, task_receiver, ret_channel_send);
    });

    // get exec type
    let command: String = arg_matches
        .value_of("exec_type")
        .unwrap_or("transfer")
        .parse()
        .expect("can not parse exec type");

    
    match command.as_str() {
        "transfer" => {
            exec_transfer(arg_matches, task_sender, ret_channel_recv);
        },
        "server" => {
            exec_server(arg_matches, task_sender, ret_channel_recv);
        },
        "local_test" => {
            exec_local_test(arg_matches, task_sender, ret_channel_recv);
        }
        _ => {println!("Undefined exec command!");}
    }

}

// transfer command syntax:
// Synopsis:
//     Connect to a transfer server listening on port <port> at node
//     <dest>, using local port <localPort>, and transfer <amount> bytes.
// Required arguments:
//     dest: address of destination node
//     port: destination port
//     localPort: local port
//     amount: number of bytes to transfer
// Optional arguments:
//     interval: execution interval of the transfer client, default 1 second
//     buf_size: buffer size of the transfer client, default 65536
fn exec_transfer (args: ArgMatches, task_sender: Sender<TaskMsg>, ret_channel_recv: Receiver<Receiver<TaskRet>>) {
    let dest_addr: String = args
        .value_of("dest_addr")
        .unwrap_or("127.0.0.1")
        .parse()
        .expect("can not parse dest addr");
    let dest_port: u8 = args
        .value_of("dest_port")
        .unwrap_or("88")
        .parse()
        .expect("can not parse dest port");
    let local_port: u8 = args
        .value_of("local_port")
        .unwrap_or("88")
        .parse()
        .expect("can not parse local port");
    let local_addr: String = args
        .value_of("local_addr")
        .unwrap_or("127.0.0.1")
        .parse()
        .expect("can not parse local addr");
    let byte_num: u32 = args
        .value_of("num_byte")
        .unwrap_or("1024")
        .parse()
        .expect("can not parse num of bytes");
    let interval: f32 = args
        .value_of("interval")
        .unwrap_or("1.0")
        .parse()
        .expect("can not parse interval");
    let buf_size: u32 = args
        .value_of("buf_size")
        .unwrap_or("65536")
        .parse()
        .expect("can not parse buffer size");

    println!("transfer args parsed.");

    // === example Java code ====
    // TCPSock sock = this.tcpMan.socket();
    // sock.bind(localPort);
    // sock.connect(destAddr, port);
    // TransferClient client = new
    //     TransferClient(manager, this, sock, amount, interval, sz);
    // client.start();

    // share with multiple socks
    let mut sock = Socket::new(local_addr, task_sender.clone(), &ret_channel_recv);
    sock.bind(local_port).expect("Can not bind local port!");
    sock.connect(dest_addr, dest_port).expect("Can not establish connection.");

    // sleep to wait for other threads to do the job
    thread::sleep(time::Duration::from_secs(5));

}


// server command syntax:
//     server port backlog [servint workint sz]
// Synopsis:
//     Start a transfer server at the local node, listening on port
//     <port>.  The server has a maximum pending (incoming) connection
//     queue of length <backlog>.
// Required arguments:
//     port: listening port
//     backlog: maximum length of pending connection queue
// Optional arguments:
// ======= TODO: Do I need this two? ======
//     servint: execution interval of the transfer server, default 1 second
//     workint: execution interval of the transfer worker, default 1 second
//     sz: buffer size of the transfer worker, default 65536
fn exec_server (args: ArgMatches, task_sender: Sender<TaskMsg>, ret_channel_recv: Receiver<Receiver<TaskRet>>) {
    let local_addr: String = args
        .value_of("local_addr")
        .unwrap_or("127.0.0.1")
        .parse()
        .expect("can not parse local addr");
    let local_port: u8 = args
        .value_of("local_port")
        .unwrap_or("88")
        .parse()
        .expect("can not parse local port");
    let backlog: u32 = args
        .value_of("backlog")
        .unwrap_or("16")
        .parse()
        .expect("can not parse backlog");
    
    println!("server args parsed.");

    // === example Java code ====
    // TCPSock sock = this.tcpMan.socket();
    // sock.bind(port);
    // sock.listen(backlog);

    // TransferServer server = new
    //    TransferServer(manager, this, sock, servint, workint, sz);
    // server.start();


    let mut sock = Socket::new(local_addr, task_sender.clone(), &ret_channel_recv);
    sock.bind(local_port).expect("Can not bind local port!");
    sock.listen(backlog).expect("Can not listen to port!");

}

fn exec_local_test (args: ArgMatches, task_sender: Sender<TaskMsg>, ret_channel_recv: Receiver<Receiver<TaskRet>>) {
    println!("It is not finished.");
}