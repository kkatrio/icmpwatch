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
enum IpcMessage {
    MakeNoise,
    StopNoise,
}

impl IpcMessage {
    fn bytes(self) -> [u8; 4] {
        match self {
            Self::MakeNoise => [1, 0, 0, 4],
            Self::StopNoise => [1, 0, 0, 5],
        }
    }
}

struct IcmpMessage {
    packet: Vec<u8>,
}

impl IcmpMessage {
    fn new(seq: u16) -> IcmpMessage {
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
            0,
            0, // checksum
            0,
            30, // identifier
            (seq >> 8) as u8,
            (seq & 0xff) as u8, // sequence
            b'a',
            b'l',
            b'i',
            b'v',
            b'e',
        ];

        Self { packet }
    }

    //fn step_packet_sequence(&mut self) {
    //    let mut seq = u16::from_be_bytes([self.packet[6], self.packet[7]]);
    //    seq += 1; // overflow
    //    self.packet[6] = (seq >> 8) as u8;
    //    self.packet[7] = (seq & 0xff) as u8;
    //}
}

// state
struct Watcher<'a> {
    socket: &'a UdpSocket,
    sequence: u16,
}

impl<'a> Watcher<'a> {
    fn new(socket: &'a UdpSocket) -> Self {
        Watcher {
            socket,
            // at start each host replies with 0
            sequence: 0,
        }
    }

    fn sleep(&self) {
        std::thread::sleep(Duration::from_secs(INTERVAL_SECS));
    }

    fn ping(&mut self, msg: IcmpMessage, target: &str) -> Result<(), PingError> {
        let start = std::time::Instant::now();

        self.socket
            .send_to(&msg.packet, (target, 0))
            .expect("unable to send packet");

        // member function
        println!(
            "> ping {} {} bytes watcher_seq: {}",
            target,
            &msg.packet.len(),
            &self.sequence
        );

        let mut buf = [0u8; 64];
        let (size, destination_addr) = match self.socket.recv_from(&mut buf) {
            Ok((sz, dest)) => (sz, dest),
            Err(..) => {
                eprintln!("Unable to receive response. Host must be down.");
                return Err(PingError::Alarm);
            }
        };

        let duration = start.elapsed();

        let check = self.validate_response(&buf, target);
        if check.is_ok() {
            // successfull print
            let sequence_received = u16::from_be_bytes([buf[6], buf[7]]);
            println!(
                "{} bytes from {} time={} ms, icmp_seq={}",
                size,
                destination_addr,
                duration.as_millis(),
                sequence_received
            );
        };
        buf.fill(0);
        check
    }

    fn validate_response(&self, buf: &[u8], target: &str) -> Result<(), PingError> {
        let type_byte = buf[0];
        // 0 Echo Reply
        if type_byte == 0 {
            let sequence_received = u16::from_be_bytes([buf[6], buf[7]]);
            // let sequence_received = ((buf[6] as u16) << 8) | (buf[7] as u16);
            if sequence_received != self.sequence {
                return Err(PingError::Io(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "{} responded with sequence {} but watcher sent {}",
                        target, sequence_received, self.sequence
                    ),
                )));
            }
            return Ok(());
        // 3 Destination Unreachable
        } else if type_byte == 3 {
            eprintln!("Destination Host Unreachable: {}", target);
        } else {
            eprintln!("received message with type: {}", type_byte);
        }
        // raise the alarm on any not sucessfull reply.
        // TODO: raise different alarm severities per case
        Err(PingError::Alarm)
    }
}

fn main() -> std::io::Result<()> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::ICMPV4))
        .expect("cannot initialize Socket");
    let socket: UdpSocket = socket.into();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();

    let f = File::open("target_hosts").expect("target_hosts file not found");
    let reader = BufReader::new(f);
    let targets: Vec<String> = reader
        .lines()
        .map(|l| l.expect("cannot create new string out of each host in the target_hosts file"))
        .collect();

    let mut watcher = Watcher::new(&socket);

    loop {
        for target in &targets {
            let msg = IcmpMessage::new(watcher.sequence);
            match watcher.ping(msg, target) {
                Ok(..) => {
                    // TODO: implement tolerance per target
                }
                Err(e) => {
                    match e {
                        PingError::Io(error) => {
                            eprintln!("{error}");
                            // do nothing for now
                        }
                        PingError::Alarm => {
                            println!("alarm! {target} is down. Sending alarm notification");
                            send_alarm_notification(IpcMessage::MakeNoise)?
                        }
                    }
                }
            }
        }
        watcher.sequence += 1;
        watcher.sleep();
    }
}

// printf "\x01\x00\x00\x05" | nc 127.0.0.1 8080
fn send_alarm_notification(message: IpcMessage) -> std::io::Result<()> {
    let mut addrs_iter = "olympus:8080".to_socket_addrs().unwrap();
    let addr = addrs_iter
        .next()
        .expect("addr must be created out of the domain");
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
    println!("connected to olympus");

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
