#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

mod dhcp_server;
mod dns_packet;
mod dns_server;
mod network;
mod web;
// mod web;

use cyw43::NetDriver;
use defmt as _;
use defmt_rtt as _;
use dhcp_server::dhcp_server_task;
use dns_server::{dns_server_task, mdns_server_task};
use embassy_net::{tcp::TcpSocket, Stack};
use embassy_time::Timer;
use embedded_io_async::Write;
use smoltcp::wire::Ipv4Address;

use panic_probe as _;
use web::start_server;
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
    let server_address = Ipv4Address::new(169, 254, 1, 1);
    let outside_address = Ipv4Address::new(198, 51, 100, 0);
    let (_, stack) = set_up_network_stack(
        &spawner,
        p.PIN_23,
        p.PIN_25,
        p.PIO0,
        p.PIN_24,
        p.PIN_29,
        p.DMA_CH0,
        server_address,
        outside_address,
    )
    .await;

    spawner.must_spawn(dhcp_server_task(stack, server_address));
    spawner.must_spawn(dns_server_task(stack, server_address, outside_address));
    spawner.must_spawn(mdns_server_task(stack, server_address, outside_address));    
    start_server(&spawner, stack).await;
    spawner.must_spawn(alive());
}
