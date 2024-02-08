use core::marker::PhantomData;

use embassy_net::udp::{PacketMetadata, UdpSocket};
use heapless::Vec;
use rand::distributions::OpenClosed01;
use smoltcp::wire::{DhcpMessageType, DhcpPacket, DhcpRepr, EthernetAddress};

enum DhcpAssignment {
    Offer {
        transaction_id: u32,
        identifier: EthernetAddress,
    },
    Acknowledged {
        identifier: EthernetAddress,
    },
}

struct DhcpServer<
    'a,
    const N_ADDRESSES: usize,
    const SERVER_PORT: u16,
    const CLIENT_PORT: u16,
    const DATA_BUFFER_LEN: usize,
> {
    addresses: Vec<DhcpAssignment, N_ADDRESSES>,
    socket: UdpSocket<'a>,
    data_buffer: [u8; DATA_BUFFER_LEN],
}

impl<
        'a,
        const N_ADDRESSES: usize,
        const SERVER_PORT: u16,
        const CLIENT_PORT: u16,
        const DATA_BUFFER_LEN: usize,
    > DhcpServer<'a, N_ADDRESSES, SERVER_PORT, CLIENT_PORT, DATA_BUFFER_LEN>
{
    fn new(mut socket: UdpSocket<'a>) -> Option<Self> {
        if socket.endpoint().is_specified() {
            None
        } else {
            socket.bind(SERVER_PORT).ok()?;
            Some(Self {
                addresses: Vec::new(),
                socket,
                data_buffer: [0u8; DATA_BUFFER_LEN],
            })
        }
    }

    fn find_ip(&self, client_address: &EthernetAddress) -> Option<usize> {
        self.addresses
            .iter()
            .enumerate()
            .find_map(|(i, a)| {
                if (match a {
                    DhcpAssignment::Offer { identifier, .. } => identifier,
                    DhcpAssignment::Acknowledged { identifier } => identifier,
                }) == client_address
                {
                    Some(i)
                } else {
                    None
                }
            })
            .or_else(|| {
                if self.addresses.len() < N_ADDRESSES {
                    Some(self.addresses.len())
                } else {
                    None
                }
            })
    }

    async fn run(&mut self) -> ! {
        loop {
            match self.socket.recv_from(&mut self.data_buffer).await {
                Ok((something, endpoint)) => {
                    let p = DhcpPacket::new_checked(&self.data_buffer).unwrap();
                    let pr = DhcpRepr::parse(&p).unwrap();
                    match pr.message_type {
                        DhcpMessageType::Discover => {
			    log::info!("{:?}", pr);
			},
                        DhcpMessageType::Offer => todo!(),
                        DhcpMessageType::Request => todo!(),
                        DhcpMessageType::Decline => todo!(),
                        DhcpMessageType::Ack => todo!(),
                        DhcpMessageType::Nak => todo!(),
                        DhcpMessageType::Release => todo!(),
                        DhcpMessageType::Inform => todo!(),
                        DhcpMessageType::Unknown(_) => todo!(),
                    }
                }
                Err(_) => {
                    log::info!("Error receiving data")
                }
            }
        }
    }
}

#[embassy_executor::task]
pub async fn dhcp_server_task(stack: &'static embassy_net::Stack<cyw43::NetDriver<'static>>) -> ! {
    let mut rx_meta = [PacketMetadata::EMPTY; 1024];
    let mut rx_buffer = [0; 1024];
    let mut tx_meta = [PacketMetadata::EMPTY; 1024];
    let mut tx_buffer = [0; 1024];

    let mut socket = embassy_net::udp::UdpSocket::new(
        stack,
        &mut rx_meta,
        &mut rx_buffer,
        &mut tx_meta,
        &mut tx_buffer,
    );

    let mut server: DhcpServer<'_, 10, 67, 68, 2048> = DhcpServer::new(socket).unwrap();

    server.run().await
}
