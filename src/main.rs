use airplay2_protocol::airplay::airplay_consumer::ArcAirPlayConsumer;
use airplay2_protocol::airplay::AirPlayConfig;
use airplay2_protocol::airplay_bonjour::AirPlayBonjour;
use airplay2_protocol::control_handle::ControlHandle;
use airplay2_protocol::net::server::Server as AirServer;
use airplay2_protocol::setup_log;
use kircast_desktop::airplay::VideoConsumer;
use std::net::SocketAddr;
use std::sync::Arc;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    setup_log();
    let port = 31927;
    let name = "RustAirplay";

    // pin码认证功能缺失...
    let _air = AirPlayBonjour::new(name, port, false);

    let addr: SocketAddr = ([0, 0, 0, 0], port).into();
    let airplay_config = AirPlayConfig {
        server_name: name.to_string(),
        width: 1920,
        height: 1080,
        fps: 30,
        port,
    };
    let video_consumer: ArcAirPlayConsumer = Arc::new(Box::<VideoConsumer>::default());
    let mserver = AirServer::bind(
        addr,
        ControlHandle::new(airplay_config, video_consumer.clone(), video_consumer),
    );
    mserver.run().await?;
    Ok(())
}
