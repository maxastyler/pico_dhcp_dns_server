use core::mem;
use smoltcp::wire::Ipv4Address;

/// a DNS header is 12 bytes
const DNS_HEADER_SIZE: usize = 12;

pub struct DnsHeader<'a> {
    buffer: &'a [u8],
}
impl<'a> DnsHeader<'a> {
    /// try to create a dns header from a buffer
    pub fn new_checked(buffer: &'a [u8]) -> Option<Self> {
        if buffer.len() < DNS_HEADER_SIZE {
            None
        } else {
            Some(Self { buffer })
        }
    }

    pub fn is_query(&self) -> bool {
        (self.buffer[2] >> 7 & 1) == 0
    }

    pub fn is_standard_query(&self) -> bool {
        (self.buffer[2] >> 3 & 0xf) == 0
    }

    pub fn question_count(&self) -> u16 {
        ((self.buffer[4] as u16) << 8) | self.buffer[5] as u16
    }
}

pub struct DnsQuestion<'a> {
    buffer: &'a [u8],
}

impl<'a> DnsQuestion<'a> {
    pub fn new_checked(buffer: &'a [u8]) -> Option<Self> {
        Self::length(buffer).map(|_| Self { buffer })
    }

    /// Given a buffer, calculate the length of the question part of the packet in bytes
    pub fn length(mut buffer: &[u8]) -> Option<usize> {
        let mut length = 0;
        while !buffer.is_empty() {
            let skip = buffer[0] as usize;
            if skip == 0 {
                return Some(length + 5);
            } else {
                length += 1 + skip;
                buffer = &buffer[1 + skip..];
            }
        }
        None
    }
}

pub struct DnsPacket<'a> {
    buffer: &'a [u8],
}

impl<'a> DnsPacket<'a> {
    pub fn new_checked(buffer: &'a [u8]) -> Option<Self> {
        Some(Self { buffer })
    }

    pub fn header<'b>(&self) -> DnsHeader<'b>
    where
        'a: 'b,
    {
        DnsHeader {
            buffer: self.buffer,
        }
    }

    /// get the `question`th question from this packet
    pub fn question<'b>(&self, question: u16) -> Option<DnsQuestion<'b>>
    where
        'a: 'b,
    {
        if question >= self.header().question_count() {
            None
        } else {
            let mut offset = DNS_HEADER_SIZE;
            let mut buffer = self.buffer.get(offset..)?;
            for _ in 0..question.saturating_sub(1) {
                let question_length = DnsQuestion::length(buffer)?;
                buffer = buffer.get(question_length..)?;
                offset += question_length;
            }
            DnsQuestion::new_checked(buffer)
        }
    }

    pub fn transform_query_to_response<'buffer>(
        query_buffer: &'buffer mut [u8],
        ip_address: Ipv4Address,
    ) -> Option<&'buffer [u8]> {
        let header = DnsHeader::new_checked(&query_buffer)?;
        if !header.is_query() {
            return None;
        }
        if !header.is_standard_query() {
            return None;
        }

        // only respond to queries with one question
        if header.question_count() != 1 {
            return None;
        }

        let question_length = DnsQuestion::length(&query_buffer)?;
        let answer_start = DNS_HEADER_SIZE + question_length;

        let ip_address_start = answer_start + 2 + 2 + 2 + 4 + 2;

        query_buffer[answer_start..ip_address_start].copy_from_slice(&[
            0b1100_0000,
            DNS_HEADER_SIZE as u8, // pointer to question
            0x00,
            0x01, // this is an address
            0x00,
            0x01, // internet class
            0,
            0,
            0,
            60, // cache time is 60 s
            0,
            4, // 4 bytes field
        ]);
        // copy the address
        query_buffer[ip_address_start..ip_address_start + 4].copy_from_slice(&ip_address.0);

        // this is a response, authoritative
        query_buffer[2] = (1 << 7) | (1 << 2);
        // authenticated
        query_buffer[3] = 1 << 7;
        // set answer count to 1
        query_buffer[6] = 0;
        query_buffer[7] = 1;
        query_buffer[8..12].copy_from_slice(&[0, 0, 0, 0]);

        Some(&query_buffer[..ip_address_start + 4])
    }
}
