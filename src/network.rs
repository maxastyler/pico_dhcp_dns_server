use cyw43::Control;
use cyw43::NetDriver;
use cyw43_pio::PioSpi;
use embassy_executor::Spawner;
use embassy_net::driver::Driver;
use embassy_net::udp::PacketMetadata;
use embassy_net::Stack;
use embassy_rp::gpio::Level;
use embassy_rp::gpio::Output;
use embassy_rp::peripherals::DMA_CH0;
use embassy_rp::peripherals::PIN_23;
use embassy_rp::peripherals::PIN_24;
use embassy_rp::peripherals::PIN_25;
use embassy_rp::peripherals::PIN_29;
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::Pio;

use embassy_rp::Peripherals;
use log::info;
use rand::Rng;
use static_cell::make_static;

use crate::Irqs;
use crate::WEB_TASK_POOL_SIZE;

#[embassy_executor::task]
async fn wifi_task(
    runner: cyw43::Runner<
        'static,
        Output<'static, PIN_23>,
        PioSpi<'static, PIN_25, PIO0, 0, DMA_CH0>,
    >,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(stack: &'static embassy_net::Stack<cyw43::NetDriver<'static>>) -> ! {
    stack.run().await
}

pub async fn set_up_network_stack(
    spawner: &Spawner,
    power_pin: PIN_23,
    cs_pin: PIN_25,
    pio_0: PIO0,
    dio: PIN_24,
    clk: PIN_29,
    dma: DMA_CH0,
) -> (Control<'static>, &'static Stack<NetDriver<'static>>) {
    let fw = include_bytes!("../firmware/43439A0.bin");
    let clm = include_bytes!("../firmware/43439A0_clm.bin");

    let pwr = Output::new(power_pin, Level::Low);
    let cs = Output::new(cs_pin, Level::High);
    let mut pio_wifi = Pio::new(pio_0, Irqs);
    let spi = cyw43_pio::PioSpi::new(
        &mut pio_wifi.common,
        pio_wifi.sm0,
        pio_wifi.irq0,
        cs,
        dio,
        clk,
        dma,
    );

    let state = make_static!(cyw43::State::new());
    let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;
    spawner.must_spawn(wifi_task(runner));
    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    let stack = &*make_static!(embassy_net::Stack::new(
        net_device,
        embassy_net::Config::ipv4_static(embassy_net::StaticConfigV4 {
            address: embassy_net::Ipv4Cidr::new(embassy_net::Ipv4Address::new(169, 254, 1, 1), 16),
            gateway: None,
            dns_servers: Default::default(),
        }),
        make_static!(embassy_net::StackResources::<WEB_TASK_POOL_SIZE>::new()),
        embassy_rp::clocks::RoscRng.gen(),
    ));

    spawner.must_spawn(net_task(stack));

    info!("Starting access point...");

    control.start_ap_open("pico", 5).await;

    (control, stack)
}
