use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_rp::{clocks::RoscRng, pio::IrqFlags};
use embassy_time::{Duration, Instant};
use heapless::Vec;
use rand::Rng;
use smoltcp::wire::{
    DhcpMessageType, DhcpOption, DhcpPacket, DhcpRepr, EthernetAddress, IpEndpoint, Ipv4Address,
    Result,
};

const OPTIONS: &[DhcpOption<'static>] = &[];

#[derive(Debug)]
enum DhcpAssignment {
    Offered {
        transaction_id: u32,
        identifier: EthernetAddress,
    },
    Assigned {
        transaction_id: u32,
        identifier: EthernetAddress,
        lease_end_time: Instant,
    },
    Free,
}

struct DhcpServer<
    'a,
    const N_ADDRESSES: usize,
    const SERVER_PORT: u16,
    const CLIENT_PORT: u16,
    const DATA_BUFFER_LEN: usize,
> {
    server_address: Ipv4Address,
    assignments: [DhcpAssignment; N_ADDRESSES],
    socket: UdpSocket<'a>,
    data_buffer: [u8; DATA_BUFFER_LEN],
    lease_time: Duration,
}

impl<
        'a,
        const N_ADDRESSES: usize,
        const SERVER_PORT: u16,
        const CLIENT_PORT: u16,
        const DATA_BUFFER_LEN: usize,
    > DhcpServer<'a, N_ADDRESSES, SERVER_PORT, CLIENT_PORT, DATA_BUFFER_LEN>
{
    fn construct_packet_repr(
        message_type: DhcpMessageType,
        server_ip: Ipv4Address,
        client_ip: Ipv4Address,
        client_hardware_address: EthernetAddress,
        transaction_id: u32,
        assigned_address: Ipv4Address,
        lease_duration_seconds: Option<u32>,
    ) -> DhcpRepr<'a> {
        DhcpRepr {
            message_type,
            transaction_id,
            secs: 0,
            client_hardware_address,
            client_ip,
            your_ip: assigned_address,
            server_ip,
            router: None,
            subnet_mask: None,
            relay_agent_ip: Ipv4Address::new(0, 0, 0, 0),
            broadcast: false,
            requested_ip: None,
            client_identifier: None,
            server_identifier: Some(server_ip),
            parameter_request_list: None,
            dns_servers: Some(Vec::from_slice(&[server_ip]).unwrap()),
            max_size: None,
            lease_duration: lease_duration_seconds,
            renew_duration: None,
            rebind_duration: None,
            additional_options: OPTIONS,
        }
    }

    fn construct_offer(
        server_ip: Ipv4Address,
        client_hardware_address: EthernetAddress,
        transaction_id: u32,
        assigned_address: Ipv4Address,
        lease_duration_seconds: u32,
    ) -> DhcpRepr<'a> {
        Self::construct_packet_repr(
            DhcpMessageType::Offer,
            server_ip,
            Ipv4Address::new(0, 0, 0, 0),
            client_hardware_address,
            transaction_id,
            assigned_address,
            Some(lease_duration_seconds),
        )
    }

    fn construct_ack(
        server_ip: Ipv4Address,
        client_hardware_address: EthernetAddress,
        client_ip: Ipv4Address,
        transaction_id: u32,
        address: Ipv4Address,
    ) -> DhcpRepr<'a> {
        Self::construct_packet_repr(
            DhcpMessageType::Ack,
            server_ip,
            client_ip,
            client_hardware_address,
            transaction_id,
            address,
            None,
        )
    }

    fn construct_nack(
        client_hardware_address: EthernetAddress,
        client_ip: Ipv4Address,
        transaction_id: u32,
    ) -> DhcpRepr<'a> {
        Self::construct_packet_repr(
            DhcpMessageType::Nak,
            Ipv4Address::new(0, 0, 0, 0),
            client_ip,
            client_hardware_address,
            transaction_id,
            Ipv4Address::new(0, 0, 0, 0),
            None,
        )
    }

    async fn construct_and_send_offer(
        &mut self,
        client_hardware_address: EthernetAddress,
        transaction_id: u32,
        index: u8,
    ) -> Result<()> {
        let mut packet = DhcpPacket::new_checked(&mut self.data_buffer)?;
        let address = Self::construct_address(index);
        let packet_repr = Self::construct_offer(
            self.server_address,
            client_hardware_address,
            transaction_id,
            address,
            self.lease_time.as_secs() as u32,
        );
        let len = packet_repr.buffer_len();

        packet_repr.emit(&mut packet)?;
        self.socket
            .send_to(
                &self.data_buffer[..len],
                IpEndpoint::new(Ipv4Address::BROADCAST.into(), CLIENT_PORT),
            )
            .await
            .map_err(|_| smoltcp::wire::Error)?;
        Ok(())
    }

    async fn construct_and_send_ack(
        &mut self,
        client_hardware_address: EthernetAddress,
        transaction_id: u32,
        index: u8,
        client_ip: Ipv4Address,
    ) -> Result<()> {
        let mut packet = DhcpPacket::new_checked(&mut self.data_buffer)?;
        let address = Self::construct_address(index);
        let packet_repr = Self::construct_ack(
            self.server_address,
            client_hardware_address,
            client_ip,
            transaction_id,
            address,
        );
        let len = packet_repr.buffer_len();

        packet_repr.emit(&mut packet)?;
        self.socket
            .send_to(
                &self.data_buffer[..len],
                IpEndpoint::new(Ipv4Address::BROADCAST.into(), CLIENT_PORT),
            )
            .await
            .map_err(|_| smoltcp::wire::Error)?;
        Ok(())
    }

    async fn construct_and_send_nack(
        &mut self,
        client_hardware_address: EthernetAddress,
        transaction_id: u32,
        client_ip: Ipv4Address,
    ) -> Result<()> {
        let mut packet = DhcpPacket::new_checked(&mut self.data_buffer)?;
        let packet_repr = Self::construct_nack(client_hardware_address, client_ip, transaction_id);
        let len = packet_repr.buffer_len();

        packet_repr.emit(&mut packet)?;
        self.socket
            .send_to(
                &self.data_buffer[..len],
                IpEndpoint::new(Ipv4Address::BROADCAST.into(), CLIENT_PORT),
            )
            .await
            .map_err(|_| smoltcp::wire::Error)?;
        Ok(())
    }

    fn new(
        mut socket: UdpSocket<'a>,
        server_address: Ipv4Address,
        lease_time: Duration,
    ) -> Option<Self> {
        if socket.endpoint().is_specified() {
            None
        } else {
            socket.bind(SERVER_PORT).ok()?;
            Some(Self {
                lease_time,
                server_address,
                assignments: [(); N_ADDRESSES].map(|_| DhcpAssignment::Free),
                socket,
                data_buffer: [0u8; DATA_BUFFER_LEN],
            })
        }
    }
    
    /// this function constructs the addresses that we'll use.
    /// TODO: probably update this???
    fn construct_address(i: u8) -> Ipv4Address {
        Ipv4Address::new(169, 254, 1, i + 2)
    }

    /// Given a discover message from a client, go through the list of addresses.
    /// If there is already an offer out to the same identifier, update the transaction_id and
    /// send a new offer out
    /// If the offer is to a different identifier, and this is the first offer like that found,
    /// make this offer be possibly overwritten
    /// If there is already an assignment, send out an offer with the assignment's address and
    /// new transaction ID
    /// If there is a free spot, create an offer in that spot and send out an offer.
    /// If there are no free spots, use the first different ID offer found.
    async fn process_discover(
        &mut self,
        client_hardware_address: EthernetAddress,
        client_identifier: Option<EthernetAddress>,
        transaction_id: u32,
    ) -> Result<()> {
        let id = client_identifier.unwrap_or(client_hardware_address);
        let mut first_offered_index = None;
        for (index, assignment) in self.assignments.iter_mut().enumerate() {
            match assignment {
                // if the lease has been offered to another index, then this can possibly be taken
                // if there are no more spots
                DhcpAssignment::Offered {
                    identifier: a_identifier,
                    ..
                } if a_identifier != &id => first_offered_index = Some(index),
                // if the assignment is given to a different identifier, and the lease time on that assignment is not up yet, then this spot can't be used
                DhcpAssignment::Assigned {
                    identifier: a_identifier,
                    lease_end_time,
                    ..
                } if (*a_identifier != id) && (Instant::now() < *lease_end_time) => {}
                _ => {
                    // we are free to use this assignment spot
                    *assignment = DhcpAssignment::Offered {
                        transaction_id,
                        identifier: id,
                    };
                    return self
                        .construct_and_send_offer(
                            client_hardware_address,
                            transaction_id,
                            index as u8,
                        )
                        .await;
                }
            }
        }
        if let Some(i) = first_offered_index {
            *unsafe { self.assignments.get_unchecked_mut(i) } = DhcpAssignment::Offered {
                identifier: id,
                transaction_id,
            };
            self.construct_and_send_offer(client_hardware_address, transaction_id, i as u8)
                .await
        } else {
            Ok(())
        }
    }

    async fn process_request(
        &mut self,
        client_ip: Ipv4Address,
        client_hardware_address: EthernetAddress,
        client_identifier: Option<EthernetAddress>,
        transaction_id: u32,
    ) -> Result<()> {
        let id = client_identifier.unwrap_or(client_hardware_address);
        let mut assigned_index = None;
        for (i, assignment) in self.assignments.iter_mut().enumerate() {
            match assignment {
                // we have offered an assignment with this transaction_id to this identifier
                DhcpAssignment::Offered {
                    identifier: a_identifier,
                    transaction_id: a_transaction_id,
                } if (*a_identifier == id) && (*a_transaction_id == transaction_id) => {
                    *assignment = DhcpAssignment::Assigned {
                        identifier: id,
                        lease_end_time: Instant::now()
                            .checked_add(self.lease_time)
                            .ok_or(smoltcp::wire::Error)?,
                        transaction_id,
                    };
                    assigned_index = Some(i);
                    break;
                }
                // we have already assigned an ip to this identifier with this transaction_id, and the
                // lease time isn't up
                DhcpAssignment::Assigned {
                    identifier: a_identifier,
                    transaction_id: a_transaction_id,
                    lease_end_time,
                } if (*a_identifier == id)
                    && (*a_transaction_id == transaction_id)
                    && (Instant::now() < *lease_end_time) =>
                {
                    *lease_end_time = Instant::now()
                        .checked_add(self.lease_time)
                        .ok_or(smoltcp::wire::Error)?;
                    assigned_index = Some(i);
                    break;
                }
                _ => {}
            }
        }
        if let Some(i) = assigned_index {
            self.construct_and_send_ack(client_hardware_address, transaction_id, i as u8, client_ip)
                .await
        } else {
            self.construct_and_send_nack(client_hardware_address, transaction_id, client_ip)
                .await
        }
    }

    async fn process_packet(&mut self) -> Result<()> {
        let packet = DhcpPacket::new_checked(&self.data_buffer)?;
        let packet_repr = DhcpRepr::parse(&packet)?;
        match packet_repr.message_type {
            DhcpMessageType::Discover => {
                self.process_discover(
                    packet_repr.client_hardware_address,
                    packet_repr.client_identifier,
                    packet_repr.transaction_id,
                )
                .await
            }
            DhcpMessageType::Request => {
                self.process_request(
                    packet_repr.client_ip,
                    packet_repr.client_hardware_address,
                    packet_repr.client_identifier,
                    packet_repr.transaction_id,
                )
                .await
            }
            _ => Ok(()),
        }
    }

    async fn run(&mut self) -> ! {
        loop {
            match self.socket.recv_from(&mut self.data_buffer).await {
                Ok((_, _)) => {
                    self.process_packet().await.unwrap();
                }
                Err(_) => {
                    log::info!("Error receiving data")
                }
            }
        }
    }
}

#[embassy_executor::task]
pub async fn dhcp_server_task(stack: &'static embassy_net::Stack<cyw43::NetDriver<'static>>, assigned_address: Ipv4Address) -> ! {
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

    let mut server: DhcpServer<'_, 10, 67, 68, 2048> = DhcpServer::new(
        socket,
	assigned_address,
        Duration::from_secs(60 * 60),
    )
    .unwrap();

    server.run().await
}
