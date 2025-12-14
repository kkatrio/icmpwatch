// The linux kernel overwrites this
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
