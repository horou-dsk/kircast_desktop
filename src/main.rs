use airplay2_protocol::airplay::airplay_consumer::ArcAirPlayConsumer;
use airplay2_protocol::airplay::AirPlayConfig;
use airplay2_protocol::airplay_bonjour::AirPlayBonjour;
use airplay2_protocol::control_handle::ControlHandle;
use airplay2_protocol::net::server::Server as AirServer;
use airplay2_protocol::setup_log;
use kircast_desktop::airplay::VideoConsumer;
use std::sync::Arc;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    setup_log();

    let name = "RustAirplay";
    let volume = 0.5;

    let airplay_config = AirPlayConfig {
        server_name: name.to_string(),
        width: 1920,
        height: 1080,
        fps: 60,
        volume,
    };
    let video_consumer: ArcAirPlayConsumer = Arc::new(VideoConsumer::default());
    let mserver = AirServer::bind_default(ControlHandle::new(
        airplay_config,
        video_consumer.clone(),
        video_consumer,
    ))
    .await;

    // pin码认证功能缺失...
    let _air = AirPlayBonjour::new(name, mserver.port, false);

    mserver.run().await?;
    Ok(())
}
