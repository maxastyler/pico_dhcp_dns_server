#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

mod dhcp_server;
mod dns_packet;
mod dns_server;
mod network;
// mod web;

use cyw43::NetDriver;
use defmt as _;
use defmt_rtt as _;
use dhcp_server::dhcp_server_task;
use dns_server::dns_server_task;
use embassy_net::{tcp::TcpSocket, Stack};
use embassy_time::Timer;
use embedded_io_async::Write;
use smoltcp::wire::Ipv4Address;

use panic_probe as _;
// use web::start_server;

use crate::network::set_up_network_stack;

const WEB_TASK_POOL_SIZE: usize = 10;

embassy_rp::bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<embassy_rp::peripherals::PIO0>;
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<embassy_rp::peripherals::USB>;
});

#[embassy_executor::task]
async fn logger_task(usb: embassy_rp::peripherals::USB) {
    let driver = embassy_rp::usb::Driver::new(usb, Irqs);
    embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
}

#[embassy_executor::task]
async fn web_task(stack: &'static Stack<NetDriver<'static>>) {
    let mut rx_buffer = [0; 1024];
    let mut tx_buffer = [0; 1024];
    log::info!("Starting tcp stack");
    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        if let Err(_) = socket.accept(80).await {
            log::warn!("Couldn't accept thing");
            continue;
        }
        log::info!("GOT INCOMING CONNECTION");
        if let Err(e) = socket
            .write_all(
                "<!DOCTYPE html>
<html>
    <head>
        <title>Example</title>
    </head>
    <body>
        <p>This is an example of a simple HTML page with one paragraph.</p>
    </body>
</html>"
                    .as_bytes(),
            )
            .await
        {
            log::info!("UH OOOH, couldn't connect, {:?}", e);
        }
    }
}

#[embassy_executor::task]
async fn alive() {
    loop {
        Timer::after_secs(4).await;
        log::info!("I'm alive");
    }
}

#[embassy_executor::main]
async fn main(spawner: embassy_executor::Spawner) {
    let p = embassy_rp::init(Default::default());

    spawner.must_spawn(logger_task(p.USB));

    let (_, stack) = set_up_network_stack(
        &spawner, p.PIN_23, p.PIN_25, p.PIO0, p.PIN_24, p.PIN_29, p.DMA_CH0,
    )
    .await;
    let server_address = Ipv4Address::new(169, 254, 1, 1);
    spawner.must_spawn(dhcp_server_task(stack, server_address));
    spawner.must_spawn(dns_server_task(stack, server_address));
    // start_server(&spawner, stack).await;
    spawner.must_spawn(web_task(stack));
    spawner.must_spawn(alive());
}
