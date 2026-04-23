use captains_log::*;
use rstest::fixture;

#[fixture]
pub fn setup_log() {
    #[cfg(feature = "trace_log")]
    {
        let format = recipe::LOG_FORMAT_THREADED_DEBUG;
        #[cfg(miri)]
        {
            let _ = std::fs::remove_file("/tmp/emb_miri.log");
            let file = LogRawFile::new("/tmp", "emb_miri.log", Level::Debug, format);
            captains_log::Builder::default().add_sink(file).test().build().expect("log setup");
        }
        #[cfg(not(miri))]
        {
            let ring = ringfile::LogRingFile::new(
                "/tmp/emb_ring.log",
                500 * 1024 * 1024,
                Level::Debug,
                format,
            );
            let mut config = Builder::default()
                .signal(signal_consts::SIGINT)
                .signal(signal_consts::SIGTERM)
                //                .tracing_global()
                .add_sink(ring)
                .add_sink(LogConsole::new(
                    ConsoleTarget::Stdout,
                    Level::Info,
                    recipe::LOG_FORMAT_DEBUG,
                ));
            config.dynamic = true;
            config.build().expect("log_setup");
        }
    }
    #[cfg(not(feature = "trace_log"))]
    {
        let _ = recipe::env_logger("LOG_FILE", "LOG_LEVEL").build().expect("log setup");
    }
}
