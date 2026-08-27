use clap::Parser;
use greenmedical_backend::config::Cli;
use greenmedical_backend::telemetry;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    telemetry::init_tracing(cli.config.log_format);
    greenmedical_backend::run(cli).await
}
