use embassy_net::udp::{PacketMetadata, UdpSocket};
use smoltcp::wire::{DnsRepr, IpEndpoint, Ipv4Address};

use crate::dns_packet::{DnsHeader, DnsPacket};

struct DNSServer<'a, const SERVER_PORT: u16, const DATA_BUFFER_LEN: usize> {
    socket: UdpSocket<'a>,
    data_buffer: [u8; DATA_BUFFER_LEN],
    primary_address: Ipv4Address,
    secondary_address: Ipv4Address,
}

impl<'a, const SERVER_PORT: u16, const DATA_BUFFER_LEN: usize>
    DNSServer<'a, SERVER_PORT, DATA_BUFFER_LEN>
{
    fn new(
        mut socket: UdpSocket<'a>,
        primary_address: Ipv4Address,
        secondary_address: Ipv4Address,
    ) -> Option<Self> {
        if socket.endpoint().is_specified() {
            None
        } else {
            socket.bind(SERVER_PORT).ok()?;
            Some(Self {
                socket,
                data_buffer: [0; DATA_BUFFER_LEN],
                primary_address,
                secondary_address,
            })
        }
    }

    async fn process_packet(
        data_buffer: &mut [u8],
        assigned_address: Ipv4Address,
        secondary_address: Ipv4Address,
    ) -> Option<&[u8]> {
        DnsPacket::transform_query_to_response(data_buffer, assigned_address, secondary_address)
    }

    async fn run(&mut self) -> ! {
        log::info!("In the run function");
        loop {
            let DNSServer {
                data_buffer,
                primary_address,
                secondary_address,
                ..
            } = self;
            match self.socket.recv_from(data_buffer).await {
                Ok((_, endpoint)) => {
                    log::info!("Got a dns packet");
                    if let Some(response_buffer) =
                        Self::process_packet(data_buffer, *primary_address, *secondary_address)
                            .await
                    {
                        log::info!("Sending response buffer: {:?}", response_buffer);
                        self.socket
                            .send_to(response_buffer, endpoint)
                            .await
                            .unwrap();
                    }
                }
                Err(_) => log::info!("Error receiving data"),
            }
        }
    }
}

#[embassy_executor::task]
pub async fn dns_server_task(
    stack: &'static embassy_net::Stack<cyw43::NetDriver<'static>>,
    primary_address: Ipv4Address,
    secondary_address: Ipv4Address,
) -> ! {
    let mut rx_meta = [PacketMetadata::EMPTY; 1024];
    let mut rx_buffer = [0; 1024];
    let mut tx_meta = [PacketMetadata::EMPTY; 1024];
    let mut tx_buffer = [0; 1024];

    let socket = embassy_net::udp::UdpSocket::new(
        stack,
        &mut rx_meta,
        &mut rx_buffer,
        &mut tx_meta,
        &mut tx_buffer,
    );

    let mut server: DNSServer<'_, 53, 2048> =
        DNSServer::new(socket, primary_address, secondary_address).unwrap();
    log::info!("RUNNING DNS SERVER");
    server.run().await
}
