use socket2::{Domain, Protocol, Socket, Type};
use std::{
    fs::File,
    io::{self, BufRead, BufReader},
    net::UdpSocket,
    time::Duration,
};

#[derive(Debug)]
enum WatchError {
    Io(io::Error),
    Alarm,
}

struct Watcher {
    socket: UdpSocket,
    packet: Vec<u8>,
    buf: Vec<u8>,
}

impl Watcher {
    fn new() -> Watcher {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::ICMPV4))
            .expect("cannot initialize Socket");
        let socket: UdpSocket = socket.into();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
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

    fn ping(&mut self, target: &str) -> Result<(), WatchError> {
        let seq = u16::from_be_bytes([self.packet[6], self.packet[7]]);
        let start = std::time::Instant::now();
        self.socket
            .send_to(&self.packet, (target, 0))
            .expect("unable to send packet");
        println!("ping {} {} bytes. seq: {}", target, self.packet.len(), seq);

        let (size, destination_addr) = self
            .socket
            .recv_from(&mut self.buf)
            .expect("error receiving response");
        let duration = start.elapsed();

        //const IP_HEADER_LEN: usize = 20;
        //let icmp_msg = &buf[IP_HEADER_LEN..];

        if size < 8 {
            return Err(WatchError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("received only {} bytes", size),
            )));
        }

        let type_byte = self.buf[0];
        let sequence_received = u16::from_be_bytes([self.buf[6], self.buf[7]]);
        // let reply_seq = ((data[6] as u16) << 8) | (data[7] as u16);
        if type_byte != 0 {
            return Err(WatchError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("received type {}.", type_byte),
            )));
        }

        if sequence_received != seq {
            return Err(WatchError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("seq {}, sequence received {}", seq, sequence_received),
            )));
        }
        // raise alarm if destination host unreachable
        if type_byte == 3 {
            return Err(WatchError::Alarm);
        }

        println!(
            "{} bytes from {} time={} ms, icmp_seq={}",
            size,
            destination_addr,
            duration.as_millis(),
            sequence_received
        );

        Ok(())
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

const MAX_TOLERANCE: u16 = 10;

fn main() -> Result<(), WatchError> {
    let mut watchdog = Watcher::new();

    let f = File::open("target_hosts").expect("target_hosts file not found");
    let reader = BufReader::new(f);
    let targets: Vec<String> = reader
        .lines()
        .map(|l| l.expect("cannot create new string out of each host in the target_hosts file"))
        .collect();

    let mut tolerance: u16 = 1;

    loop {
        for host in &targets {
            // TODO: make them async, but without futures
            // keeping the state of each target host
            match watchdog.ping(host) {
                Ok(()) => {}
                Err(e) => match e {
                    WatchError::Alarm => {
                        eprintln!("Alarm!");
                        tolerance += 1;
                    }
                    WatchError::Io(ioerr) => {
                        eprintln!("io error: {}", ioerr);
                    }
                },
            };
            watchdog.reset_buf();
        }

        // TODO: reset tolerance
        if tolerance >= MAX_TOLERANCE {
            todo!()
            //watchdog.bark()
        }
        watchdog.step_packet_sequence();
        std::thread::sleep(Duration::from_secs(1));
    }
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
