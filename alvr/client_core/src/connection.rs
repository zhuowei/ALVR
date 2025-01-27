#![allow(clippy::if_same_then_else)]

use crate::{
    decoder::{self, DECODER_INIT_CONFIG},
    platform,
    sockets::AnnouncerSocket,
    statistics::StatisticsManager,
    storage::Config,
    ClientCoreEvent, CONTROL_CHANNEL_SENDER, DISCONNECT_NOTIFIER, EVENT_QUEUE, IS_ALIVE,
    IS_RESUMED, IS_STREAMING, STATISTICS_MANAGER,
};
use alvr_audio::AudioDevice;
use alvr_common::{
    glam::UVec2,
    once_cell::sync::Lazy,
    parking_lot::{Mutex, RwLock},
    prelude::*,
    ALVR_VERSION,
};
use alvr_packets::{
    ClientConnectionResult, ClientControlPacket, ClientStatistics, Haptics, ServerControlPacket,
    StreamConfigPacket, Tracking, VideoPacketHeader, VideoStreamingCapabilities, AUDIO, HAPTICS,
    STATISTICS, TRACKING, VIDEO,
};
use alvr_session::{settings_schema::Switch, SessionConfig};
use alvr_sockets::{
    spawn_cancelable, PeerType, ProtoControlSocket, ReceiverBuffer, StreamSender,
    StreamSocketBuilder,
};
use futures::future::BoxFuture;
use serde_json as json;
use std::{collections::HashMap, future, sync::Arc, thread, time::Duration};
use tokio::{runtime::Runtime, sync::mpsc as tmpsc, time};

#[cfg(target_os = "android")]
use crate::audio;
#[cfg(not(target_os = "android"))]
use alvr_audio as audio;

const INITIAL_MESSAGE: &str = concat!(
    "Searching for streamer...\n",
    "Open ALVR on your PC then click \"Trust\"\n",
    "next to the client entry",
);
const NETWORK_UNREACHABLE_MESSAGE: &str = "Cannot connect to the internet";
// const INCOMPATIBLE_VERSIONS_MESSAGE: &str = concat!(
//     "Streamer and client have\n",
//     "incompatible types.\n",
//     "Please update either the app\n",
//     "on the PC or on the headset",
// );
const STREAM_STARTING_MESSAGE: &str = "The stream will begin soon\nPlease wait...";
const SERVER_RESTART_MESSAGE: &str = "The streamer is restarting\nPlease wait...";
const SERVER_DISCONNECTED_MESSAGE: &str = "The streamer has disconnected.";

const DISCOVERY_RETRY_PAUSE: Duration = Duration::from_millis(500);
const RETRY_CONNECT_MIN_INTERVAL: Duration = Duration::from_secs(1);
const NETWORK_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(1);
const CONNECTION_RETRY_INTERVAL: Duration = Duration::from_secs(1);

pub static CONNECTION_RUNTIME: Lazy<RwLock<Option<Runtime>>> = Lazy::new(|| RwLock::new(None));
pub static TRACKING_SENDER: Lazy<Mutex<Option<StreamSender<Tracking>>>> =
    Lazy::new(|| Mutex::new(None));
pub static STATISTICS_SENDER: Lazy<Mutex<Option<StreamSender<ClientStatistics>>>> =
    Lazy::new(|| Mutex::new(None));

fn set_hud_message(message: &str) {
    let message = format!(
        "ALVR v{}\nhostname: {}\nIP: {}\n\n{message}",
        *ALVR_VERSION,
        Config::load().hostname,
        platform::local_ip(),
    );

    EVENT_QUEUE
        .lock()
        .push_back(ClientCoreEvent::UpdateHudMessage(message));
}

pub fn connection_lifecycle_loop(
    recommended_view_resolution: UVec2,
    supported_refresh_rates: Vec<f32>,
) -> IntResult {
    set_hud_message(INITIAL_MESSAGE);

    loop {
        check_interrupt!(IS_ALIVE.value());

        if IS_RESUMED.value() {
            if let Err(e) =
                connection_pipeline(recommended_view_resolution, supported_refresh_rates.clone())
            {
                match e {
                    InterruptibleError::Interrupted => return Ok(()),
                    InterruptibleError::Other(_) => {
                        let message =
                            format!("Connection error:\n{e}\nCheck the PC for more details");
                        error!("{message}");
                        set_hud_message(&message);
                    }
                }
            }
        } else {
            debug!("Skip try connection because the device is sleeping");
        }

        thread::sleep(CONNECTION_RETRY_INTERVAL);
    }
}

fn connection_pipeline(
    recommended_view_resolution: UVec2,
    supported_refresh_rates: Vec<f32>,
) -> IntResult {
    let runtime = Runtime::new().map_err(to_int_e!())?;

    let (mut proto_control_socket, server_ip) = {
        let config = Config::load();
        let announcer_socket = AnnouncerSocket::new(&config.hostname).map_err(to_int_e!())?;
        let listener_socket = runtime
            .block_on(alvr_sockets::get_server_listener())
            .map_err(to_int_e!())?;

        loop {
            check_interrupt!(IS_ALIVE.value());

            if let Err(e) = announcer_socket.broadcast() {
                warn!("Broadcast error: {e}");

                set_hud_message(NETWORK_UNREACHABLE_MESSAGE);

                thread::sleep(RETRY_CONNECT_MIN_INTERVAL);

                set_hud_message(INITIAL_MESSAGE);

                return Ok(());
            }

            let maybe_pair = runtime.block_on(async {
                tokio::select! {
                    maybe_pair = ProtoControlSocket::connect_to(PeerType::Server(&listener_socket)) => {
                        maybe_pair.map_err(to_int_e!())
                    },
                    _ = time::sleep(DISCOVERY_RETRY_PAUSE) => Err(InterruptibleError::Interrupted)
                }
            });

            if let Ok(pair) = maybe_pair {
                break pair;
            }
        }
    };

    let microphone_sample_rate = AudioDevice::new_input(None)
        .unwrap()
        .input_sample_rate()
        .unwrap();

    runtime
        .block_on(
            proto_control_socket.send(&ClientConnectionResult::ConnectionAccepted {
                client_protocol_id: alvr_common::protocol_id(),
                display_name: platform::device_model(),
                server_ip,
                streaming_capabilities: Some(VideoStreamingCapabilities {
                    default_view_resolution: recommended_view_resolution,
                    supported_refresh_rates,
                    microphone_sample_rate,
                }),
            }),
        )
        .map_err(to_int_e!())?;
    let config_packet = runtime.block_on(async {
        tokio::select! {
            res = proto_control_socket.recv::<StreamConfigPacket>() => res.map_err(to_int_e!()),
            _ = time::sleep(Duration::from_secs(1)) => int_fmt_e!("Timeout waiting for stream config"),
        }
    })?;

    let settings = {
        let mut session_desc = SessionConfig::default();
        session_desc
            .merge_from_json(&json::from_str(&config_packet.session).map_err(to_int_e!())?)
            .map_err(to_int_e!())?;
        session_desc.to_settings()
    };

    let negotiated_config =
        json::from_str::<HashMap<String, json::Value>>(&config_packet.negotiated)
            .map_err(to_int_e!())?;

    let view_resolution = negotiated_config
        .get("view_resolution")
        .and_then(|v| json::from_value(v.clone()).ok())
        .unwrap_or(UVec2::ZERO);
    let refresh_rate_hint = negotiated_config
        .get("refresh_rate_hint")
        .and_then(|v| v.as_f64())
        .unwrap_or(60.0) as f32;
    let game_audio_sample_rate = negotiated_config
        .get("game_audio_sample_rate")
        .and_then(|v| v.as_u64())
        .unwrap_or(44100) as u32;

    let streaming_start_event = ClientCoreEvent::StreamingStarted {
        view_resolution,
        refresh_rate_hint,
        settings: Box::new(settings.clone()),
    };

    *STATISTICS_MANAGER.lock() = Some(StatisticsManager::new(
        settings.connection.statistics_history_size,
        Duration::from_secs_f32(1.0 / refresh_rate_hint),
        if let Switch::Enabled(config) = settings.headset.controllers {
            config.steamvr_pipeline_frames
        } else {
            0.0
        },
    ));

    let (control_sender, mut control_receiver) = proto_control_socket.split();
    let control_sender = Arc::new(tokio::sync::Mutex::new(control_sender));

    match runtime.block_on(async {
        tokio::select! {
            res = control_receiver.recv() => res,
            _ = time::sleep(Duration::from_millis(1)) => fmt_e!("Timeout"),
        }
    }) {
        Ok(ServerControlPacket::StartStream) => {
            info!("Stream starting");
            set_hud_message(STREAM_STARTING_MESSAGE);
        }
        Ok(ServerControlPacket::Restarting) => {
            info!("Server restarting");
            set_hud_message(SERVER_RESTART_MESSAGE);
            return Ok(());
        }
        Err(e) => {
            info!("Server disconnected. Cause: {e}");
            set_hud_message(SERVER_DISCONNECTED_MESSAGE);
            return Ok(());
        }
        _ => {
            info!("Unexpected packet");
            set_hud_message("Unexpected packet");
            return Ok(());
        }
    }

    let listen_for_server_future = StreamSocketBuilder::listen_for_server(
        settings.connection.stream_port,
        settings.connection.stream_protocol,
        settings.connection.client_send_buffer_bytes,
        settings.connection.client_recv_buffer_bytes,
    );
    let stream_socket_builder = runtime.block_on(async {
        tokio::select! {
            res = listen_for_server_future => res.map_err(to_int_e!()),
            _ = time::sleep(Duration::from_millis(1)) => int_fmt_e!("Timeout while binding stream socket"),
        }
    })?;

    if let Err(e) = runtime.block_on(async {
        control_sender
            .lock()
            .await
            .send(&ClientControlPacket::StreamReady)
            .await
    }) {
        info!("Server disconnected. Cause: {e}");
        set_hud_message(SERVER_DISCONNECTED_MESSAGE);
        return Ok(());
    }

    let accept_from_server_future = stream_socket_builder.accept_from_server(
        server_ip,
        settings.connection.stream_port,
        settings.connection.packet_size as _,
    );
    let stream_socket = runtime.block_on(async {
        tokio::select! {
            res = accept_from_server_future => res.map_err(to_int_e!()),
            _ = time::sleep(Duration::from_secs(2)) => int_fmt_e!("Timeout while setting up streams")
        }
    })?;
    let stream_socket = Arc::new(stream_socket);

    info!("Connected to server");

    // create this before initializing the stream on cpp side
    let (control_channel_sender, mut control_channel_receiver) = tmpsc::unbounded_channel();
    *CONTROL_CHANNEL_SENDER.lock() = Some(control_channel_sender);

    {
        let config = &mut *DECODER_INIT_CONFIG.lock();

        config.max_buffering_frames = settings.video.max_buffering_frames;
        config.buffering_history_weight = settings.video.buffering_history_weight;
        config.options = settings.video.mediacodec_extra_options;
    }

    let tracking_sender = stream_socket.request_stream(TRACKING);
    let statistics_sender = stream_socket.request_stream(STATISTICS);
    let mut video_receiver =
        runtime.block_on(stream_socket.subscribe_to_stream::<VideoPacketHeader>(VIDEO));
    let mut haptics_receiver =
        runtime.block_on(stream_socket.subscribe_to_stream::<Haptics>(HAPTICS));

    let game_audio_loop: BoxFuture<_> = if let Switch::Enabled(config) = settings.audio.game_audio {
        let device = AudioDevice::new_output(None, None).map_err(to_int_e!())?;

        let game_audio_receiver = runtime.block_on(stream_socket.subscribe_to_stream(AUDIO));
        Box::pin(audio::play_audio_loop(
            device,
            2,
            game_audio_sample_rate,
            config.buffering,
            game_audio_receiver,
        ))
    } else {
        Box::pin(future::pending())
    };

    let microphone_loop: BoxFuture<_> = if matches!(settings.audio.microphone, Switch::Enabled(_)) {
        let device = AudioDevice::new_input(None).map_err(to_int_e!())?;

        let microphone_sender = stream_socket.request_stream(AUDIO);
        Box::pin(audio::record_audio_loop(
            device,
            1,
            false,
            microphone_sender,
        ))
    } else {
        Box::pin(future::pending())
    };

    // Important: To make sure this is successfully unset when stopping streaming, the rest of the
    // function MUST be infallible
    IS_STREAMING.set(true);
    *CONNECTION_RUNTIME.write() = Some(runtime);
    *TRACKING_SENDER.lock() = Some(tracking_sender);
    *STATISTICS_SENDER.lock() = Some(statistics_sender);

    EVENT_QUEUE.lock().push_back(streaming_start_event);

    let video_receive_loop = async move {
        let mut receiver_buffer = ReceiverBuffer::new();
        let mut stream_corrupted = false;
        loop {
            video_receiver.recv_buffer(&mut receiver_buffer).await?;
            let (header, nal) = receiver_buffer.get()?;

            if !IS_RESUMED.value() {
                break Ok(());
            }

            if let Some(stats) = &mut *STATISTICS_MANAGER.lock() {
                stats.report_video_packet_received(header.timestamp);
            }

            if header.is_idr {
                stream_corrupted = false;
            } else if receiver_buffer.had_packet_loss() {
                stream_corrupted = true;
                if let Some(sender) = &*CONTROL_CHANNEL_SENDER.lock() {
                    sender.send(ClientControlPacket::RequestIdr).ok();
                }
                warn!("Network dropped video packet");
            }

            if !stream_corrupted || !settings.connection.avoid_video_glitching {
                if !decoder::push_nal(header.timestamp, nal) {
                    stream_corrupted = true;
                    if let Some(sender) = &*CONTROL_CHANNEL_SENDER.lock() {
                        sender.send(ClientControlPacket::RequestIdr).ok();
                    }
                    warn!("Dropped video packet. Reason: Decoder saturation")
                }
            } else {
                warn!("Dropped video packet. Reason: Waiting for IDR frame")
            }
        }
    };

    let haptics_receive_loop = async move {
        loop {
            let haptics = haptics_receiver.recv_header_only().await?;

            EVENT_QUEUE.lock().push_back(ClientCoreEvent::Haptics {
                device_id: haptics.device_id,
                duration: haptics.duration,
                frequency: haptics.frequency,
                amplitude: haptics.amplitude,
            });
        }
    };

    // Poll for events that need a constant thread (mainly for the JNI env)
    #[cfg(target_os = "android")]
    thread::spawn(|| {
        use std::time::Instant;

        const BATTERY_POLL_INTERVAL: Duration = Duration::from_secs(5);

        let mut previous_hmd_battery_status = (0.0, false);
        let mut battery_poll_deadline = Instant::now();

        let battery_manager = platform::android::BatteryManager::new();

        while IS_STREAMING.value() {
            if battery_poll_deadline < Instant::now() {
                let new_hmd_battery_status = battery_manager.status();

                if new_hmd_battery_status != previous_hmd_battery_status {
                    if let Some(sender) = &*CONTROL_CHANNEL_SENDER.lock() {
                        sender
                            .send(ClientControlPacket::Battery(crate::BatteryPacket {
                                device_id: *alvr_common::HEAD_ID,
                                gauge_value: new_hmd_battery_status.0,
                                is_plugged: new_hmd_battery_status.1,
                            }))
                            .ok();

                        previous_hmd_battery_status = new_hmd_battery_status;
                    }
                }

                battery_poll_deadline += BATTERY_POLL_INTERVAL;
            }

            thread::sleep(Duration::from_millis(500));
        }
    });

    let keepalive_sender_loop = {
        let control_sender = Arc::clone(&control_sender);
        async move {
            loop {
                let res = control_sender
                    .lock()
                    .await
                    .send(&ClientControlPacket::KeepAlive)
                    .await;
                if let Err(e) = res {
                    info!("Server disconnected. Cause: {e}");
                    set_hud_message(SERVER_DISCONNECTED_MESSAGE);
                    break Ok(());
                }

                time::sleep(NETWORK_KEEPALIVE_INTERVAL).await;
            }
        }
    };

    let control_send_loop = async move {
        while let Some(packet) = control_channel_receiver.recv().await {
            control_sender.lock().await.send(&packet).await.ok();
        }

        Ok(())
    };

    let control_receive_loop = async move {
        loop {
            match control_receiver.recv().await {
                Ok(ServerControlPacket::InitializeDecoder(config)) => {
                    decoder::create_decoder(config);
                }
                Ok(ServerControlPacket::Restarting) => {
                    info!("{SERVER_RESTART_MESSAGE}");
                    set_hud_message(SERVER_RESTART_MESSAGE);
                    break Ok(());
                }
                Ok(_) => (),
                Err(e) => {
                    info!("{SERVER_DISCONNECTED_MESSAGE} Cause: {e}");
                    set_hud_message(SERVER_DISCONNECTED_MESSAGE);
                    break Ok(());
                }
            }
        }
    };

    let receive_loop = async move { stream_socket.receive_loop().await };

    let res = CONNECTION_RUNTIME.read().as_ref().unwrap().block_on(async {
        // Run many tasks concurrently. Threading is managed by the runtime, for best performance.
        tokio::select! {
            res = spawn_cancelable(receive_loop) => {
                if let Err(e) = res {
                    info!("Server disconnected. Cause: {e}");
                }
                set_hud_message(
                    SERVER_DISCONNECTED_MESSAGE
                );

                Ok(())
            },
            res = spawn_cancelable(game_audio_loop) => res,
            res = spawn_cancelable(microphone_loop) => res,
            res = spawn_cancelable(video_receive_loop) => res,
            res = spawn_cancelable(haptics_receive_loop) => res,
            res = spawn_cancelable(control_send_loop) => res,

            // keep these loops on the current task
            res = keepalive_sender_loop => res,
            res = control_receive_loop => res,

            _ = DISCONNECT_NOTIFIER.notified() => Ok(()),
        }
    });

    IS_STREAMING.set(false);
    CONNECTION_RUNTIME.write().take();
    TRACKING_SENDER.lock().take();
    STATISTICS_SENDER.lock().take();

    EVENT_QUEUE
        .lock()
        .push_back(ClientCoreEvent::StreamingStopped);

    #[cfg(target_os = "android")]
    {
        *crate::decoder::DECODER_ENQUEUER.lock() = None;
        *crate::decoder::DECODER_DEQUEUER.lock() = None;
    }

    res.map_err(to_int_e!())
}
