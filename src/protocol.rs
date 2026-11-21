pub const REPORT_SIZE: usize = 65;
const MARKER: u8 = 0xaa;

pub fn encode(command: u8, data: &[u8]) -> [u8; REPORT_SIZE] {
    let length = (1 + data.len()) as u8;
    let mut frame = vec![1, 0, MARKER, 0x12, length, command];
    let mut sum = 0x12u8.wrapping_add(length).wrapping_add(command);
    for (offset, &byte) in data.iter().enumerate() {
        frame.push(byte);
        // The first data byte is excluded by WebLink's stuffing rule.
        if offset > 0 && byte == MARKER {
            frame.push(byte);
        }
        sum = sum.wrapping_add(byte);
    }
    frame.push(sum);
    if sum == MARKER {
        frame.push(sum);
    }
    frame[1] = (frame.len() - 2) as u8;
    let mut report = [0; REPORT_SIZE];
    report[..frame.len()].copy_from_slice(&frame);
    report
}

pub fn decode(report: &[u8]) -> Result<(u8, Vec<u8>), &'static str> {
    if report.len() < 7 || report[0] != 2 || report[2] != MARKER || report[3] != 0x21 {
        return Err("unexpected HID response header");
    }
    let end = usize::from(report[1]) + 2;
    let payload_len = usize::from(report[4]).checked_sub(1).ok_or("missing command")?;
    if end > report.len() || end < 7 {
        return Err("invalid HID response length");
    }
    let mut plain = Vec::with_capacity(payload_len + 1);
    let mut bytes = report[6..end].iter().peekable();
    while let Some(&byte) = bytes.next() {
        plain.push(byte);
        if byte == MARKER && bytes.peek() == Some(&&MARKER) {
            bytes.next();
        }
    }
    let checksum = plain.pop().ok_or("missing checksum")?;
    if plain.len() != payload_len {
        return Err("invalid response payload length");
    }
    let expected = report[3..6].iter().chain(&plain)
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    if expected != checksum {
        return Err("invalid response checksum");
    }
    Ok((report[5], plain))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_reports() {
        assert_eq!(&encode(0xe0, &[])[..7], &[1, 5, 0xaa, 0x12, 1, 0xe0, 0xf3]);
        let capture = [
            0x02, 0x29, 0xaa, 0x21, 0x25, 0xc3, 0, 0, 0x65, 0, 0, 0x60, 9, 0,
            0, 0x88, 0x13, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 1, 1,
            0x24, 0, 0, 1, 0xb0, 0x14, 0, 0, 0x65,
        ];
        let (command, data) = decode(&capture).unwrap();
        assert_eq!(command, 0xc3);
        assert_eq!(&data[5..11], &[0x60, 9, 0, 0, 0x88, 0x13]);
        let mut invalid = capture;
        invalid[10] = 1;
        assert!(decode(&invalid).is_err());
    }

    #[test]
    fn escaped_payload_and_checksum() {
        let payload = [2, 7, 0xaa, 0x21, 2, 0xc9, 0xaa, 0xaa, 0x96];
        assert_eq!(decode(&payload).unwrap(), (0xc9, vec![0xaa]));
        let checksum = [2, 7, 0xaa, 0x21, 2, 0xc9, 0xbe, 0xaa, 0xaa];
        assert_eq!(decode(&checksum).unwrap(), (0xc9, vec![0xbe]));
    }
}
