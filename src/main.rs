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

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub text: String,
    pub username: String, 
    pub ip: [u8;4],
    pub port: u16,
}

impl Message {
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
    message.ip.iter().for_each(|part| {
        message_buffer.push(*part);
    });
    let port_arr: [u8;2] = split_u16(message.port);
    message_buffer.append(&mut port_arr.to_vec());
    message_buffer.push(message.username.len() as u8);
    message_buffer.append(&mut message.username.as_bytes().to_vec());
    let text_len_arr: [u8;2] = split_u16(message.text.len() as u16);
    message_buffer.append(&mut text_len_arr.to_vec());
    message_buffer.append(&mut message.text.as_bytes().to_vec());
    
    message_buffer
}

/// # Turns a `Vec<u8>` buffer into a message struct instance
pub fn decypher_message(message: &Vec<u8>) -> Message {
    let mut message_out: Message = Message::new_empty();
    let mut temp_ip: [u8;4] = [0;4];
    for (i,msg_byte) in message[0..=3].iter().enumerate() {
        temp_ip[i] = *msg_byte;
    }

    message_out.ip = temp_ip;
    message_out.port = combine_bytes([message[4], message[5]]);

    let username_len = message[6];
    let mut temp_username: String = String::new();
    for username_byte in message[7..(7+username_len) as usize].iter() {
        temp_username.push(*username_byte as char);
    }

    // let text_len: usize = combine_bytes([message[(7+username_len) as usize], message[(7+username_len+1) as usize]]) as usize;
    let mut temp_text: String = String::new();
    for text_byte in message[(9+username_len as usize)..].iter() {
        temp_text.push(*text_byte as char);
    }

    message_out.username = temp_username;
    message_out.text = temp_text;

    message_out
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
    let client_list = clients.lock().expect("failed to lock mutex");
    let mut client = client_list.get(&message.ip).ok_or(Err::<(), std::io::Error>(std::io::Error::new(std::io::ErrorKind::Other, "client not found"))).unwrap();
    let message_buffer: Vec<u8> = generate_message(message);
    let buffer_len: usize = message_buffer.len();
    client.write_all(&buffer_len.to_be_bytes())?;
    client.write_all(&message_buffer)?;
    client.flush()?;

    Ok(())
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
    pub fn start_receive(&mut self) -> Result<(), Error> {
        for stream in self.listener.incoming() {
            let clients = Arc::clone(&self.clients);
            thread::spawn(move|| -> Result<(), Error>{
                let stream = stream?;
                let stream_clone = stream.try_clone()?;
                let socket_addr_v4: SocketAddrV4 = match stream.peer_addr()? {
                    SocketAddr::V4(addr) => addr,
                    SocketAddr::V6(_) => return Err(std::io::Error::new(std::io::ErrorKind::Other, "ipv6 not supported")),
                };
                clients.lock().expect("locking mutex failed").insert(socket_addr_v4.ip().octets(), stream_clone);
                let message: Message = decypher_message(&handle_client(stream)?);
                relay(&clients, message)?;
                
                Ok(())
            });
        }
        Ok(())
    }
}

fn main() {
    let mut server: Server = Server::start(3333, None).unwrap();
    server.start_receive().unwrap();
}
