use clap::{App, Arg, ArgMatches};
use std::{sync::{mpsc::{Sender, Receiver}}, thread, time};
use my_tcp::{core::manager::{self, TaskMsg, TaskRet}, core::socket::Socket};

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

    // get exec type
    let command: String = arg_matches
        .value_of("exec_type")
        .unwrap_or("window_test")
        .parse()
        .expect("can not parse exec type");

    let args = parse_args(arg_matches);

    // ===== start socket manager ====
    let (mut socket_manager, task_sender, ret_channel_recv) = manager::SocketManager::new(args.local_addr.clone());
    thread::spawn(move || {
        socket_manager.start();
    });
    
    match command.as_str() {
        "transfer" => {
            exec_transfer(args, task_sender, ret_channel_recv);
        },
        "server" => {
            exec_server(args, task_sender, ret_channel_recv);
        },
        "local_test" => {
            exec_local_test(args, task_sender, ret_channel_recv);
        },
        "window_test" => {
            exec_window_test(args, task_sender, ret_channel_recv);
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
fn exec_transfer (args: NodeArgs, task_sender: Sender<TaskMsg>, ret_channel_recv: Receiver<Receiver<TaskRet>>) {

    println!("transfer args parsed.");

    // === example Java code ====
    // TCPSock sock = this.tcpMan.socket();
    // sock.bind(localPort);
    // sock.connect(destAddr, port);
    // TransferClient client = new
    //     TransferClient(manager, this, sock, amount, interval, sz);
    // client.start();


    let mut sock1 = Socket::new(args.local_addr.clone(), task_sender.clone(), &ret_channel_recv);
    sock1.bind(args.local_port).expect("Can not bind local port!");
    sock1.connect(args.dest_addr.clone(), args.dest_port).expect("Can not establish connection.");

    // wait
    thread::sleep(time::Duration::from_millis(1000));

    let mut sock2 = Socket::new(args.local_addr, task_sender.clone(), &ret_channel_recv);
    sock2.bind(args.local_port + 1).expect("Can not bind local port!");
    sock2.connect(args.dest_addr, args.dest_port).expect("Can not establish connection.");

    thread::sleep(time::Duration::from_millis(1000));

    for _ in 0..20 {
        let mut test_data = Vec::new();
        for i in 0..2000 {
            test_data.push((i % 200) as u8);
        }
        sock1.write_all(&test_data).unwrap();
        sock2.write_all(&test_data).unwrap();
        // quicker sender
        thread::sleep(time::Duration::from_millis(5));
    }

    sock1.close();
    thread::sleep(time::Duration::from_secs(10));
    sock2.close();

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
fn exec_server (args: NodeArgs, task_sender: Sender<TaskMsg>, ret_channel_recv: Receiver<Receiver<TaskRet>>) {
    
    println!("server args parsed.");

    // === example Java code ====
    // TCPSock sock = this.tcpMan.socket();
    // sock.bind(port);
    // sock.listen(backlog);

    // TransferServer server = new
    //    TransferServer(manager, this, sock, servint, workint, sz);
    // server.start();


    let mut sock = Socket::new(args.local_addr, task_sender.clone(), &ret_channel_recv);
    sock.bind(args.local_port).expect("Can not bind local port!");
    sock.listen(args.backlog).expect("Can not listen to port!");

    loop {
        // the receiving socket at server
        let server_recv =  sock.accept();
        if server_recv.is_ok() {
            let server_recv = server_recv.unwrap();
            let remote = format!("{}:{}", server_recv.id.remote_addr, server_recv.id.remote_port);
            println!("Got a new connection from {}", remote);
            thread::spawn(move || {
                for _ in 0..20 {
                    let recv_data = server_recv.read_all(2000).unwrap();       
                    for i in 0..2000 {
                        if recv_data[i] != (i % 200) as u8 {
                            println!("From {}: Wring data received !!!", remote);
                            return;
                        }
                    }
                    // wait
                    thread::sleep(time::Duration::from_millis(10));
                }
                println!("From {}: All data right! \n", remote);
            });
        }
        // wait
        thread::sleep(time::Duration::from_millis(100));
    }

}

fn exec_local_test (args: NodeArgs, task_sender: Sender<TaskMsg>, ret_channel_recv: Receiver<Receiver<TaskRet>>) {
    println!("Local Test started ...");
    // start server socket
    let mut server_sock = Socket::new(args.local_addr.clone(), task_sender.clone(), &ret_channel_recv);
    server_sock.bind(args.local_port).expect("Can not bind local port!");
    server_sock.listen(args.backlog).expect("Can not listen to port!");

    let mut client_sock = Socket::new(args.local_addr.clone(), task_sender.clone(), &ret_channel_recv);
    client_sock.bind(args.local_port + 1).expect("Can not bind local port!");
    client_sock.connect(args.local_addr, args.local_port).expect("Can not establish connection.");

    // wait for the connection to be established
    thread::sleep(time::Duration::from_millis(100));

    // the receiving socket at server
    let server_recv =  server_sock.accept().expect("Can not get connection!");

    // interleaved test
    for _ in 0..20 {
        let mut test_data = Vec::new();
        for i in 0..200 {
            test_data.push(i as u8);
        }
        let test_data = client_sock.write(&test_data, 0, 200).unwrap();
        println!("Len = {} wrote ", test_data);

        // wait
        thread::sleep(time::Duration::from_millis(100));

        let recv_data = server_recv.read(200).unwrap();

        for i in 0..200 {
            assert!(recv_data[i] == i as u8);
        }
    }

    // send and recv test
    for _ in 0..20 {
        let mut test_data = Vec::new();
        for i in 0..200 {
            test_data.push(i as u8);
        }
        let test_data = client_sock.write(&test_data, 0, 200).unwrap();
        println!("Len = {} wrote ", test_data);
    }

    // wait
    thread::sleep(time::Duration::from_millis(200));

    for _ in 0..20 {
        let recv_data = server_recv.read_all(200).unwrap();

        for i in 0..200 {
            assert!(recv_data[i] == i as u8);
        }
    }

    println!("All data right! \n");

    client_sock.close();
    // server_recv.close(); // we do not support current FIN from both side.
    server_sock.close();

    // wait
    thread::sleep(time::Duration::from_millis(10));
    server_recv.release();

    // sleep to wait for other threads to do the job
    thread::sleep(time::Duration::from_secs(10));
    
}

fn exec_window_test (args: NodeArgs, task_sender: Sender<TaskMsg>, ret_channel_recv: Receiver<Receiver<TaskRet>>) {
    println!("Window Test started ...");
    // start server socket
    let mut server_sock = Socket::new(args.local_addr.clone(), task_sender.clone(), &ret_channel_recv);
    server_sock.bind(args.local_port).expect("Can not bind local port!");
    server_sock.listen(args.backlog).expect("Can not listen to port!");

    let mut client_sock = Socket::new(args.local_addr.clone(), task_sender.clone(), &ret_channel_recv);
    client_sock.bind(args.local_port + 1).expect("Can not bind local port!");
    client_sock.connect(args.local_addr, args.local_port).expect("Can not establish connection.");

    // wait for the connection to be established
    thread::sleep(time::Duration::from_millis(100));

    // the receiving socket at server
    let server_recv =  server_sock.accept().expect("Can not get connection!");

    // flow control test
    thread::spawn(move || {
        for _ in 0..40 {
            let mut test_data = Vec::new();
            for i in 0..200 {
                test_data.push(i as u8);
            }
            let test_data = client_sock.write(&test_data, 0, 200).unwrap();
            println!("Len = {} wrote ", test_data);
            // quick sender
            thread::sleep(time::Duration::from_millis(10));
        }
        thread::sleep(time::Duration::from_secs(10));
        client_sock.release();
    });

    for _ in 0..40 {
        let recv_data = server_recv.read_all(200).unwrap();
        // slow receiver!
        thread::sleep(time::Duration::from_millis(200));

        for i in 0..200 {
            assert!(recv_data[i] == i as u8);
        }
    }

    println!("All data right! \n");
    thread::sleep(time::Duration::from_millis(10));
    server_recv.close(); // we do not support current FIN from both side.
    thread::sleep(time::Duration::from_millis(10));
    server_sock.release();

    // wait
    thread::sleep(time::Duration::from_millis(10));

    // sleep to wait for other threads to do the job
    thread::sleep(time::Duration::from_secs(10));
    
}


#[derive(Default)]
struct NodeArgs {
    // for client
    dest_addr: String,
    dest_port: u8,
    local_addr: String,
    local_port: u8,
    byte_num: u32,
    interval: f32,
    /// buf size of node apps 
    buf_size: u32,
    backlog: u32,
}

fn parse_args (arg_matches: ArgMatches) -> NodeArgs {
    let mut args = NodeArgs::default();

    args.dest_addr = arg_matches
        .value_of("dest_addr")
        .unwrap_or("127.0.0.1")
        .parse()
        .expect("can not parse dest addr");
    args.dest_port = arg_matches
        .value_of("dest_port")
        .unwrap_or("88")
        .parse()
        .expect("can not parse dest port");
    args.local_port = arg_matches
        .value_of("local_port")
        .unwrap_or("88")
        .parse()
        .expect("can not parse local port");
    args.local_addr = arg_matches
        .value_of("local_addr")
        .unwrap_or("127.0.0.1")
        .parse()
        .expect("can not parse local addr");
    args.byte_num = arg_matches
        .value_of("num_byte")
        .unwrap_or("1024")
        .parse()
        .expect("can not parse num of bytes");
    args.interval = arg_matches
        .value_of("interval")
        .unwrap_or("1.0")
        .parse()
        .expect("can not parse interval");
    args.buf_size = arg_matches
        .value_of("buf_size")
        .unwrap_or("65536")
        .parse()
        .expect("can not parse buffer size");
    // for server
    args.backlog = arg_matches
        .value_of("backlog")
        .unwrap_or("16")
        .parse()
        .expect("can not parse backlog");

    return args;
}