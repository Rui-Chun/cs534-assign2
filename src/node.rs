use clap::{App, Arg, ArgMatches};
use std::env;
use my_tcp::app;

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
        .unwrap_or("transfer")
        .parse()
        .expect("can not parse exec type");
    
    match command.as_str() {
        "transfer" => {
            exec_transfer(arg_matches);
        },
        "server" => {
            exec_server(arg_matches);
        },
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
fn exec_transfer (args: ArgMatches) {
    let dest: String = args
        .value_of("dest_addr")
        .unwrap_or("127.0.0.1")
        .parse()
        .expect("can not parse dest addr");
    let dest_port: u8 = args
        .value_of("dest_port")
        .unwrap_or("88")
        .parse()
        .expect("can not parse dest port");
    let local_port: String = args
        .value_of("dest_addr")
        .unwrap_or("127.0.0.1")
        .parse()
        .expect("can not parse dest addr");
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
fn exec_server (args: ArgMatches) {
    let local_port: String = args
        .value_of("dest_addr")
        .unwrap_or("127.0.0.1")
        .parse()
        .expect("can not parse dest addr");
    let backlog: u16 = args
        .value_of("backlog")
        .unwrap_or("32")
        .parse()
        .expect("can not parse backlog");
    
    println!("server args parsed.");

}
