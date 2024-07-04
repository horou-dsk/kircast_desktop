use airplay2_protocol::airplay::airplay_consumer::ArcAirPlayConsumer;
use airplay2_protocol::airplay::AirPlayConfigBuilder;
use airplay2_protocol::airplay_bonjour::AirPlayBonjour;
use airplay2_protocol::control_handle::ControlHandle;
use airplay2_protocol::net::server::Server as AirServer;
use kircast_desktop::airplay::VideoConsumer;
use kircast_desktop::log_conf::init_tracing_subscriber;
use std::sync::Arc;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let (_out, _err) = init_tracing_subscriber(&["kircast_desktop"], Some(Level::INFO));
    log_panics::init();

    let name = "RustAirplay";
    let volume = 0.5;
    let pin_pwd = "1234";

    let airplay_config = AirPlayConfigBuilder::new(name.to_string())
        .width(1920)
        .height(1080)
        .fps(60)
        .volume(volume)
        .audio_buffer_size(24)
        .pin_pwd(pin_pwd)
        .build();
    let video_consumer: ArcAirPlayConsumer = Arc::new(VideoConsumer::default());
    let mserver = AirServer::bind_default(ControlHandle::new(
        airplay_config,
        video_consumer.clone(),
        video_consumer,
    ))
    .await;

    let _air = AirPlayBonjour::new(name, mserver.port, true);

    info!(
        "Airplay 投屏服务开启成功，投屏名称： {}，投屏密码： {}",
        name, pin_pwd
    );

    mserver.run().await?;
    Ok(())
}
