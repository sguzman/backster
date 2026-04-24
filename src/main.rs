mod wolfram;
mod data;
mod features;
mod strategy;
mod backtest;
mod report;
mod config;
mod modes;
mod optimizer;
mod stats;
mod cache;

fn main() -> anyhow::Result<()> {
    init_tracing();

    let cfg_path = parse_args_for_config_path()?;
    let cfg_str = std::fs::read_to_string(&cfg_path)?;
    let cfg: crate::config::AppConfig = toml::from_str(&cfg_str)?;

    match cfg.mode {
        crate::config::Mode::Backtest => {
            let bt = cfg.backtest.ok_or_else(|| anyhow::anyhow!("Missing [backtest] config"))?;
            crate::modes::backtest::run_backtest(&bt, &cfg.data)?;
        }
        crate::config::Mode::Optimize => {
            let opt = cfg
                .optimizer
                .ok_or_else(|| anyhow::anyhow!("Missing [optimizer] config"))?;
            crate::modes::optimize::run_optimize(&opt, &cfg.data)?;
        }
    }

    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .compact()
        .without_time()
        .with_env_filter(filter)
        .with_ansi(true)
        .with_target(false)
        .with_writer(std::io::stdout)
        .init();
}

fn parse_args_for_config_path() -> anyhow::Result<std::path::PathBuf> {
    let mut args = std::env::args().skip(1);
    let mut cfg: Option<std::path::PathBuf> = None;

    while let Some(a) = args.next() {
        // Accept `backster config <file.toml>` as a convenience.
        if a == "config" && cfg.is_none() {
            cfg = args.next().map(std::path::PathBuf::from);
            continue;
        }
        if a == "--config" || a == "-c" {
            cfg = args.next().map(std::path::PathBuf::from);
        } else if cfg.is_none() {
            cfg = Some(std::path::PathBuf::from(a));
        }
    }

    cfg.ok_or_else(|| anyhow::anyhow!("Usage: backster --config <config.toml>"))
}
