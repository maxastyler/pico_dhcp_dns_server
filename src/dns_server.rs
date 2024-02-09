use embassy_net::udp::{PacketMetadata, UdpSocket};
use smoltcp::wire::{DnsRepr, IpEndpoint, Result};

struct DNSServer<'a, const SERVER_PORT: u16, const DATA_BUFFER_LEN: usize> {
    socket: UdpSocket<'a>,
    data_buffer: [u8; DATA_BUFFER_LEN],
}

impl<'a, const SERVER_PORT: u16, const DATA_BUFFER_LEN: usize>
    DNSServer<'a, SERVER_PORT, DATA_BUFFER_LEN>
{
    fn new(mut socket: UdpSocket<'a>) -> Option<Self> {
        if socket.endpoint().is_specified() {
            None
        } else {
            socket.bind(SERVER_PORT).ok()?;
            Some(Self {
                socket,
                data_buffer: [0; DATA_BUFFER_LEN],
            })
        }
    }

    async fn process_packet(&mut self, sender: IpEndpoint) -> Result<()> {
        log::info!("\n\nnew data!!!\n\n{:?}\n\n", self.data_buffer);
        Ok(())
    }

    async fn run(&mut self) -> ! {
        loop {
            match self.socket.recv_from(&mut self.data_buffer).await {
                Ok((_, endpoint)) => {
		    log::info!("GOT SOCKET");
                    self.process_packet(endpoint).await;
                }
                Err(_) => log::info!("Error receiving data"),
            }
        }
    }
}

#[embassy_executor::task]
pub async fn dns_server_task(stack: &'static embassy_net::Stack<cyw43::NetDriver<'static>>) -> ! {
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

    let mut server: DNSServer<'_, 53, 2048> = DNSServer::new(socket).unwrap();
    log::info!("RUNNING DNS SERVER");
    server.run().await
}
