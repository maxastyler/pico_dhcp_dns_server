use cyw43::NetDriver;
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_time::Duration;
use picoserve::{
    response::{
        self, status::TEMPORARY_REDIRECT, IntoResponse, Json, Redirect, Response, StatusCode,
    },
    routing::{get, Layer, PathRouter},
    ResponseSent, Router,
};
use static_cell::make_static;

pub const WEB_TASK_POOL_SIZE: usize = 3;

struct EmbassyTimer;

impl picoserve::Timer for EmbassyTimer {
    type Duration = embassy_time::Duration;
    type TimeoutError = embassy_time::TimeoutError;

    async fn run_with_timeout<F: core::future::Future>(
        &mut self,
        duration: Self::Duration,
        future: F,
    ) -> Result<F::Output, Self::TimeoutError> {
        embassy_time::with_timeout(duration, future).await
    }
}

type AppRouter = impl PathRouter;

#[embassy_executor::task(pool_size = WEB_TASK_POOL_SIZE)]
async fn web_task(
    id: usize,
    stack: &'static embassy_net::Stack<cyw43::NetDriver<'static>>,
    app: &'static picoserve::Router<AppRouter>,
    config: &'static picoserve::Config<Duration>,
) -> ! {
    let mut rx_buffer = [0; 1024];
    let mut tx_buffer = [0; 1024];

    loop {
        let mut socket = embassy_net::tcp::TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);

        log::info!("{id}: Listening on TCP:80...");
        if let Err(e) = socket.accept(80).await {
            log::warn!("{id}: accept error: {:?}", e);
            continue;
        }

        log::info!(
            "{id}: Received connection from {:?}",
            socket.remote_endpoint()
        );

        let (socket_rx, socket_tx) = socket.split();
        match picoserve::serve(
            app,
            EmbassyTimer,
            config,
            &mut [0; 2048],
            socket_rx,
            socket_tx,
        )
        .await
        {
            Ok(handled_requests_count) => {
                log::info!(
                    "{handled_requests_count} requests handled from {:?}",
                    socket.remote_endpoint()
                );
            }
            Err(err) => log::error!("{err:?}"),
        }
    }
}

struct S;

impl<State, PathParameters> Layer<State, PathParameters> for S {
    type NextState = State;

    type NextPathParameters = PathParameters;

    async fn call_layer<
        NextLayer: picoserve::routing::Next<Self::NextState, Self::NextPathParameters>,
        W: response::ResponseWriter,
    >(
        &self,
        next: NextLayer,
        state: &State,
        path_parameters: PathParameters,
        request: picoserve::request::Request<'_>,
        response_writer: W,
    ) -> Result<ResponseSent, W::Error> {
        if request
            .headers()
            .get("Host")
            .map_or(false, |h| h == "169.254.1.1")
        {
            response_writer
                .write_response(Json("hi").into_response())
                .await
        } else {
            Redirect::to("169.254.1.1").write_to(response_writer).await
        }
    }
}
fn make_app() -> picoserve::Router<AppRouter> {
    Router::new().layer(S)
}

pub async fn start_server(spawner: &Spawner, stack: &'static Stack<NetDriver<'static>>) {
    let app = make_static!(make_app());

    let config = make_static!(picoserve::Config::new(picoserve::Timeouts {
        start_read_request: Some(Duration::from_secs(5)),
        read_request: Some(Duration::from_secs(1)),
        write: Some(Duration::from_secs(1)),
    })
    .keep_connection_alive());

    for id in 0..WEB_TASK_POOL_SIZE {
        spawner.must_spawn(web_task(id, stack, app, config));
    }
}
