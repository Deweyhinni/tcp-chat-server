// chat server
use std::net::{TcpStream, TcpListener, SocketAddrV4, Ipv4Addr, SocketAddr};
use std::collections::HashMap;
use std::io::{Error, Read, Write};
use std::sync::Mutex;
use std::sync::Arc;
use std::thread;

/// # Splits a u16 into an array of two u8
fn split_u16(short_u16: u16) -> [u8;2] {
    let high_byte: u8 = (short_u16 >> 8) as u8;
    let low_byte: u8 = (short_u16 & 0xff) as u8;

    return [high_byte, low_byte];
}

/// # Turns an array of two u8 into a single u16
fn combine_bytes(bytes: [u8;2]) -> u16 {
    let short_u16 = ((bytes[0] as u16) << 8) | bytes[1] as u16;
    short_u16
}

pub enum Message {
    ErrorMsg(ErrorMsg),
    SuccessMsg(SuccessMsg),
    TextMsg(TextMsg),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ErrorMsg {
    pub error_text: String,
    pub error_code: ErrorCode,
    pub username: String, 
    pub ip: [u8;4],
    pub port: u16,
}

impl ErrorMsg {
    const MSG_TYPE: u8 = 0;

    pub fn new(error_text: String, error_code: ErrorCode, username: String, ip: &[u8;4], port: &u16,) -> Self {
        Self {
            error_text,
            error_code,
            username,
            ip: *ip,
            port: *port,
        }
    }
    
    pub fn new_empty() -> Self {
        Self {
            error_text: "".to_string(),
            error_code: ErrorCode::ServerError,
            username: "".to_string(),
            ip: [0,0,0,0],
            port: 0,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorCode {
    ServerError = 0,
    ClientNotFound = 1,
    MessageError = 2,
}

pub struct SuccessMsg {
    pub text: String,
    pub username: String, 
    pub ip: [u8;4],
    pub port: u16,
}

impl SuccessMsg {
    const MSG_TYPE: u8 = 1;

    pub fn new(text: String, username: String, ip: &[u8;4], port: &u16,) -> Self {
        Self {
            text,
            username,
            ip: *ip,
            port: *port,
        }
    }
    
    pub fn new_empty() -> Self {
        Self {
            text: "".to_string(),
            username: "".to_string(),
            ip: [0,0,0,0],
            port: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextMsg {
    pub text: String,
    pub username: String, 
    pub ip: [u8;4],
    pub port: u16,
}

impl TextMsg {
    const MSG_TYPE: u8 = 2;

    pub fn new(text: String, username: String, ip: &[u8;4], port: &u16,) -> Self {
        Self {
            text,
            username,
            ip: *ip,
            port: *port,
        }
    }
    
    pub fn new_empty() -> Self {
        Self {
            text: "".to_string(),
            username: "".to_string(),
            ip: [0,0,0,0],
            port: 0,
        }
    }
}

/// # Generates the `Vec<u8>` buffer to send through the websocket from a message struct instance
/// first 4 bytes are ip, next two are port next byte is the length of the username in bytes and
/// then the the username and then length of the message text in bytes is two bytes
/// - bytes 0-3: ip
/// - bytes 4-5: port
/// - byte 6: username length
/// - username, max 255 bytes
/// - two bytes, text length
/// - rest: text theoretical max 65535
pub fn generate_message(message: Message) -> Vec<u8> {
    let mut message_buffer: Vec<u8> = Vec::new();
    match message {
        Message::ErrorMsg(msg) => {
            message_buffer.push(0);
        }
        Message::SuccessMsg(msg) => {
            message_buffer.push(1);
        }
        Message::TextMsg(msg) => {
            message_buffer.push(2);
            msg.ip.iter().for_each(|part| {
                message_buffer.push(*part);
            });
            let port_arr: [u8;2] = split_u16(msg.port);
            message_buffer.append(&mut port_arr.to_vec());
            message_buffer.push(msg.username.len() as u8);
            message_buffer.append(&mut msg.username.as_bytes().to_vec());
            let text_len_arr: [u8;2] = split_u16(msg.text.len() as u16);
            message_buffer.append(&mut text_len_arr.to_vec());
            message_buffer.append(&mut msg.text.as_bytes().to_vec());
        }
    }
    
    message_buffer
}

#[derive(Debug)]
pub enum ParsingError {
    MsgTypeErr,
    MsgStructureErr,
}

/// # Turns a `Vec<u8>` buffer into a message struct instance
pub fn decypher_message(message: &Vec<u8>) -> Result<Message, ParsingError> {
    match message[0] {
        0 => {
            todo!()
        }
        1 => {
            let mut message_out: SuccessMsg = SuccessMsg::new_empty();
            

            Ok(Message::SuccessMsg(message_out))
        }
        2 => {
            let mut message_out: TextMsg = TextMsg::new_empty();
            let mut temp_ip: [u8;4] = [0;4];
            for (i,msg_byte) in message[1..=4].iter().enumerate() {
                temp_ip[i] = *msg_byte;
            }

            message_out.ip = temp_ip;
            message_out.port = combine_bytes([message[5], message[6]]);

            let username_len = message[7];
            let mut temp_username: String = String::new();
            for username_byte in message[8..(8+username_len) as usize].iter() {
                temp_username.push(*username_byte as char);
            }

            // let text_len: usize = combine_bytes([message[(8+username_len) as usize], message[(8+username_len+1) as usize]]) as usize;
            let mut temp_text: String = String::new();
            for text_byte in message[(10+username_len as usize)..].iter() {
                temp_text.push(*text_byte as char);
            }

            message_out.username = temp_username;
            message_out.text = temp_text;

            Ok(Message::TextMsg(message_out))
        }
        _  => Err(ParsingError::MsgTypeErr)
    }
}

/// # Takes a `TcpStream`, reads the data and returns a `Vec<u8>` buffer
/// it first reads the length of the message, creates a Vec of that length and uses `read_exact()`
/// to read to that buffer and returns it.
fn handle_client(mut input_stream: TcpStream) -> Result<Vec<u8>, Error> {
    let peer = input_stream.peer_addr();
    println!("connection from: {}", peer.unwrap());
    let mut len_bytes = [0_u8;8];
    input_stream.read_exact(&mut len_bytes)?;
    let msg_len: usize = u64::from_be_bytes(len_bytes) as usize;
    let mut buffer: Vec<u8> = vec![0;msg_len];
    input_stream.read_exact(&mut buffer)?;
    println!("{:?}", buffer);
    Ok(buffer)
}

/// # Takes in a `HashMap` of streams and tries to find the right recipient and sends the message.
/// it takes a reference to a `Arc<Mutex<HashMap<[u8;4],TcpStream>>>` a map of all connected
/// clients and a Message and tries to find the right recipient based off the ip in the message
/// then uses the `TcpStream` to send the message to the client by first sending the length of the
/// message and then the content.
fn relay(clients: &Arc<Mutex<HashMap<[u8;4],TcpStream>>>, message: Message) -> Result<(), Error> {
    match message {
        Message::ErrorMsg(_err_msg) => {
            todo!("im not sure why you would wanna relay an error message tbh");
        }
        Message::SuccessMsg(msg) => {
            todo!()
        }
        Message::TextMsg(msg) => {
            let client_list = clients.lock().expect("failed to lock mutex");
            let mut client = client_list.get(&msg.ip).ok_or(Err::<(), std::io::Error>(std::io::Error::new(std::io::ErrorKind::Other, "client not found"))).unwrap();
            let message_buffer: Vec<u8> = generate_message(Message::TextMsg(msg));
            let buffer_len: u64 = message_buffer.len() as u64;
            client.write_all(&buffer_len.to_be_bytes())?;
            client.write_all(&message_buffer)?;
            client.flush()?;
        }
    }

    Ok(())
}

fn relay_loop(input_stream: TcpStream, clients: &Arc<Mutex<HashMap<[u8;4],TcpStream>>>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    loop {
        let mut input_stream_clone = input_stream.try_clone()?;
        let msg_buff = handle_client(input_stream_clone)?;
        let message: Message = decypher_message(&msg_buff).expect("decyphering message failed");
        relay(&clients, message)?;

    }
}

pub struct Server {
    clients: Arc<Mutex<HashMap<[u8;4],TcpStream>>>,
    listener: TcpListener,
}

impl Server {
    /// # creates a server instance and binds listener
    pub fn start(port: u16, address: Option<Ipv4Addr>) -> Result<Self, Error> {
        Ok( Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            listener: TcpListener::bind(SocketAddrV4::new(address.unwrap_or(Ipv4Addr::new(0,0,0,0)), port))?
        })
    }

    /// # Starts listening and handles connections in new threads
    pub fn start_receive(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for stream in self.listener.incoming() {
            let clients = Arc::clone(&self.clients);
            thread::spawn(move|| -> Result<(), Box<dyn std::error::Error + Send + Sync>>{
                let stream = stream?;
                let stream_clone = stream.try_clone()?;
                let socket_addr_v4: SocketAddrV4 = match stream.peer_addr()? {
                    SocketAddr::V4(addr) => addr,
                    SocketAddr::V6(_) => panic!("ipv6 not supported"),
                };
                clients.lock().expect("locking mutex failed").insert(socket_addr_v4.ip().octets(), stream_clone);
                relay_loop(stream, &clients)
            });
        }
        Ok(())
    }
}

fn main() {
    let mut server: Server = Server::start(3333, None).unwrap();
    server.start_receive().unwrap();
}
