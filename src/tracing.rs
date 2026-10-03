use std::io::{self, IsTerminal, Write};
use std::sync::OnceLock;

use indicatif::MultiProgress;
use tracing_subscriber::{fmt::MakeWriter, prelude::*};

static MULTI: OnceLock<MultiProgress> = OnceLock::new();

struct SuspendingWriter;

impl Write for SuspendingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // clear the bars, write the log line, redraw the bars
        multi_progress().suspend(|| io::stderr().write_all(buf))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stderr().flush()
    }
}

struct MakeSuspendingWriter;

impl<'a> MakeWriter<'a> for MakeSuspendingWriter {
    type Writer = SuspendingWriter;

    fn make_writer(&'a self) -> Self::Writer {
        SuspendingWriter
    }
}

pub fn multi_progress() -> &'static MultiProgress {
    MULTI.get_or_init(MultiProgress::new)
}

pub fn enable_tidlers_tracing() {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_level(true)
        .with_writer(MakeSuspendingWriter)
        .with_ansi(io::stderr().is_terminal());

    let filter_layer = tracing_subscriber::filter::EnvFilter::from_default_env()
        .add_directive("tidlers=debug".parse().unwrap());

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .init();
}
