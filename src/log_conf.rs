use std::io::{stderr, stdout};

use tracing::{Level, Metadata};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    fmt::time::FormatTime,
    layer::{self, Filter, SubscriberExt as _},
    util::SubscriberInitExt as _,
    Layer as _,
};

struct LocalTimer;

impl FormatTime for LocalTimer {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        write!(w, "[{}]", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))
    }
}

type FilterVec<T> = smallvec::SmallVec<[T; 8]>;

struct TargetFilter {
    targets: FilterVec<&'static str>,
    level: Level,
}

impl<S> Filter<S> for TargetFilter {
    fn enabled(&self, metadata: &Metadata<'_>, _: &layer::Context<'_, S>) -> bool {
        let level = metadata.level();
        let target = metadata.target();
        &self.level >= level
            && level >= &Level::WARN
            && self.targets.iter().any(|t| target.starts_with(t))
    }
}

pub fn init_tracing_subscriber(
    targets: &'static [&'static str],
    level: Option<Level>,
) -> (WorkerGuard, WorkerGuard) {
    let (non_blocking_out, guard_out) = tracing_appender::non_blocking(stdout());
    let (non_blocking_err, guard_err) = tracing_appender::non_blocking(stderr());
    let log_level = level.unwrap_or(Level::DEBUG);
    let default_targets = &["tracing_log", "panic"];
    let out_filter = {
        let mut filter_targets: FilterVec<&'static str> = FilterVec::new(); //targets.into_iter().collect();
        filter_targets.extend_from_slice(targets);
        filter_targets.extend_from_slice(default_targets);
        TargetFilter {
            targets: filter_targets,
            level: log_level,
        }
    };
    let err_filter = tracing_subscriber::filter::Targets::new().with_targets(
        targets
            .iter()
            .chain(default_targets)
            .map(|target| (*target, Level::ERROR)),
    );
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_timer(LocalTimer)
                .with_line_number(true)
                .with_target(true)
                .with_writer(non_blocking_out)
                .compact()
                .with_filter(out_filter),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_timer(LocalTimer)
                .with_line_number(true)
                .with_target(true)
                .with_writer(non_blocking_err)
                .compact()
                .with_filter(err_filter),
        )
        .init();
    (guard_out, guard_err)
}
