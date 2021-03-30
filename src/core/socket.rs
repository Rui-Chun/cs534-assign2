


// TCP socket states
#[derive(PartialEq)]
enum SocketState {
    // protocol states
    CLOSED,
    LISTEN,
    SYN_SENT,
    ESTABLISHED,
    SHUTDOWN // close requested, FIN not sent (due to unsent data in queue)
}

struct Socket {
    state: SocketState,

}

impl Socket {
    /*
     * The following are the socket APIs of TCP transport service.
     * All APIs are NON-BLOCKING.
     */

    /**
     * Bind a socket to a local port
     *
     * @param localPort int local port number to bind the socket to
     * @return int 0 on success, -1 otherwise
     */
    pub fn bind(localPort: u8) -> Result<(), isize> {

        return Err(-1);
    }

    /**
     * Listen for connections on a socket
     * the socket turns into a server socket
     * @param backlog int Maximum number of pending connections
     * @return int 0 on success, -1 otherwise
     */
    pub fn listen(backlog: u32) -> Result<(), isize> {
        
        return Err(-1);
    }

    /**
     * Accept a connection on a socket
     * Non-blocking?
     * @return TCPSock The first established connection on the request queue
     */
    pub fn accept() -> Result<Self, isize> {

        return Err(-1); // no pending connection
    }

    /**
     * Initiate connection to a remote socket
     *
     * @param destAddr int Destination node address
     * @param destPort int Destination port
     * @return int 0 on success, -1 otherwise
     */
    pub fn connect(destAddr: String, destPort: u8) -> Result<(), isize> {
        return Err(-1);
    }

    /**
     * Initiate closure of a connection (graceful shutdown)
     */
    pub fn close() {
    }

    /**
     * Release a connection immediately (abortive shutdown)
     */
    pub fn release() {
    }

    /**
     * Write to the socket up to len bytes from the buffer buf starting at
     * position pos.
     *
     * @param buf byte[] the buffer to write from
     * @param pos int starting position in buffer
     * @param len int number of bytes to write
     * @return int on success, the number of bytes written, which may be smaller
     *             than len; on failure, -1
     */
    pub fn write(buf: Vec<u8>, pos: u32, len: u32) -> Result<(), isize> {
        return Err(-1);
    }

    /**
     * Read from the socket up to len bytes into the buffer buf starting at
     * position pos.
     *
     * @param buf byte[] the buffer
     * @param pos int starting position in buffer
     * @param len int number of bytes to read
     * @return int on success, the number of bytes read, which may be smaller
     *             than len; on failure, -1
     */
    pub fn read(buf: &mut Vec<u8>, pos: u32, len: u32) -> Result<(), isize> {
        return Err(-1);
    }

    pub fn isConnectionPending(&self) -> bool {
        return self.state == SocketState::SYN_SENT;
    }

    pub fn isClosed(&self) -> bool {
        return self.state == SocketState::CLOSED;
    }

    pub fn isConnected(&self) -> bool {
        return self.state == SocketState::ESTABLISHED;
    }

    pub fn isClosurePending(&self) -> bool {
        return self.state == SocketState::SHUTDOWN;
    }

    /*
     * End of socket API
     */
}