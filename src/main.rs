use socket2::{Domain, Protocol, Socket, Type};
use std::net::{TcpStream, ToSocketAddrs};
use std::{
    fs::File,
    io::{self, BufRead, BufReader, Read, Write},
    net::UdpSocket,
    time::Duration,
};

const INTERVAL_SECS: u64 = 2;

#[derive(Debug)]
enum PingError {
    Io(io::Error),
    Alarm,
}

struct Watcher {
    socket: UdpSocket,
    packet: Vec<u8>,
    buf: Vec<u8>,
}

/*
 * type # bytes
 * health check # 1001
 * turn on lights # 1002
 * turn off lights # 1003
 * make noise # 1004
 * stop noise # 1005
 * start loud alarm # 1006
 * stop loud alarm # 1007
 */
enum Message {
    MakeNoise,
    StopNoise,
}

impl Message {
    fn bytes(self) -> [u8; 4] {
        match self {
            Self::MakeNoise => [1, 0, 0, 4],
            Self::StopNoise => [1, 0, 0, 5],
        }
    }
}

impl Watcher {
    fn new() -> Watcher {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::ICMPV4))
            .expect("cannot initialize Socket");
        let socket: UdpSocket = socket.into();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();

        // https://www.rfc-editor.org/rfc/rfc792
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |     Type      |     Code      |          Checksum             |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |           Identifier          |        Sequence Number        |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |     Data ...
        // +-+-+-+-+-
        let packet: Vec<u8> = vec![
            8, // type
            0, // code
            0, 0, // checksum
            0, 30, // identifier
            0, 0, // sequence,
            b'a', b'l', b'i', b'v', b'e',
        ];

        Self {
            socket,
            packet,
            buf: vec![0u8; 64],
        }
    }

    fn ping(&mut self, target: &str) -> Result<(), PingError> {
        let seq = u16::from_be_bytes([self.packet[6], self.packet[7]]);
        let start = std::time::Instant::now();
        self.socket
            .send_to(&self.packet, (target, 0))
            .expect("unable to send packet");
        println!("ping {} {} bytes. seq: {}", target, self.packet.len(), seq);

        let (size, destination_addr) = match self.socket.recv_from(&mut self.buf) {
            Ok((sz, dest)) => (sz, dest),
            Err(..) => {
                eprintln!("Unable to receive response. Host must be down.");
                return Err(PingError::Alarm);
            }
        };

        let duration = start.elapsed();

        // basic validation check
        if size < 8 {
            return Err(PingError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("received only {} bytes", size),
            )));
        }

        // 0 Echo Reply
        // 3 Destination Unreachable
        let type_byte = self.buf[0];

        if type_byte == 0 {
            // check messages sequence
            let sequence_received = u16::from_be_bytes([self.buf[6], self.buf[7]]);
            // let reply_seq = ((data[6] as u16) << 8) | (data[7] as u16);
            if sequence_received != seq {
                return Err(PingError::Io(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("seq {}, sequence received {}", seq, sequence_received),
                )));
            }
            // successfull print
            println!(
                "{} bytes from {} time={} ms, icmp_seq={}",
                size,
                destination_addr,
                duration.as_millis(),
                sequence_received
            );
            return Ok(());
        } else if type_byte == 3 {
            println!(
                "icmp_seq= {} Destination Host Unreachable. Destination: {}",
                seq, destination_addr
            );
        } else {
            println!("received message with type: {}", type_byte);
        }
        // raise the alarm on any not sucessfull reply.
        // TODO: raise differect alarm severities per case
        Err(PingError::Alarm)
    }

    fn step_packet_sequence(&mut self) {
        let mut seq = u16::from_be_bytes([self.packet[6], self.packet[7]]);
        seq += 1; // overflow
        self.packet[6] = (seq >> 8) as u8;
        self.packet[7] = (seq & 0xff) as u8;
    }

    fn reset_buf(&mut self) {
        self.buf.fill(0);
    }
}

fn main() {
    let mut watchdog = Watcher::new();

    let f = File::open("target_hosts").expect("target_hosts file not found");
    let reader = BufReader::new(f);
    let targets: Vec<String> = reader
        .lines()
        .map(|l| l.expect("cannot create new string out of each host in the target_hosts file"))
        .collect();

    //#[derive(PartialEq)]
    //enum Status {
    //    Alive,
    //    Down,
    //}

    //let mut status_map: HashMap<String, Status> = HashMap::with_capacity(targets.len());

    loop {
        for host in &targets {
            // TODO: make them async, but without futures
            // keeping the state of each target host
            match watchdog.ping(host) {
                Ok(()) => {
                    // update status
                    //if let Some(status) = status_map.get_mut(host) {
                    //    *status = Status::Alive;
                    //}
                }
                Err(e) => match e {
                    PingError::Alarm => {
                        println!("Alarm! sending notification for host: {}", host);
                        //status_map.insert(host.to_string(), Status::Down);
                        if send_alarm_notification(Message::MakeNoise).is_err() {
                            eprintln!("sending alarm notification returned error!");
                        }
                    }
                    PingError::Io(ioerr) => {
                        eprintln!("io error: {}", ioerr);
                    }
                },
            };
            watchdog.reset_buf();
        }

        watchdog.step_packet_sequence();
        std::thread::sleep(Duration::from_secs(INTERVAL_SECS));
    }
}

// printf "\x01\x00\x00\x05" | nc 127.0.0.1 8080
fn send_alarm_notification(message: Message) -> std::io::Result<()> {
    let mut addrs_iter = "olympus:8080".to_socket_addrs().unwrap();
    let addr = addrs_iter
        .next()
        .expect("addr must be created out of the domain");
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
    println!("done connecting");

    stream.write_all(&message.bytes())?;
    let mut buf = [0u8; 2];
    let _ = stream.read(&mut buf)?;
    if buf != "ok".as_bytes() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "response is not ok",
        ));
    }
    Ok(())
}

// The linux kernel calculates this anyway
#[allow(dead_code)]
fn calculate_checksum(msg: &[u8]) -> u16 {
    // The checksum is the 16-bit ones's complement of the one's
    // complement sum of the ICMP message starting with the ICMP Type.
    // For computing the checksum , the checksum field should be zero.
    // If the total length is odd, the received data is padded with one
    // octet of zeros for computing the checksum.
    let mut sum: u32 = 0;
    let chunks = msg.chunks_exact(2);
    let remainder = chunks.remainder();

    for chunk in chunks {
        let word = u16::from_be_bytes([chunk[0], chunk[1]]) as u32;
        sum += word;
    }

    assert!(remainder.len() == 1);
    let word = u16::from_be_bytes([remainder[0], 0]) as u32;
    sum += word;

    while (sum >> 16) != 0 {
        // last 16 bits + the first 16 bits that overflow
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    //one's compliment
    !sum as u16
}
//let checksum = calculate_checksum(&packet);
//dbg!(checksum);
// first 8 bits
//packet[2] = (checksum >> 8) as u8;
//// last 8 bits
//packet[3] = (checksum & 0xff) as u8;
