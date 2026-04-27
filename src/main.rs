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
    let cli = parse_args()?;
    init_tracing(cli.quiet);

    let cfg_path = cli.config_path;
    let cfg_str = std::fs::read_to_string(&cfg_path)?;
    let cfg: crate::config::AppConfig = toml::from_str(&cfg_str)?;

    match cfg.mode {
        crate::config::Mode::Backtest => {
            let bt = cfg.backtest.ok_or_else(|| anyhow::anyhow!("Missing [backtest] config"))?;
            let mut results = Vec::with_capacity(cli.n_times);
            for _ in 0..cli.n_times {
                let pct = crate::modes::backtest::run_backtest(
                    &bt,
                    &cfg.data,
                    cli.quiet,
                )?;
                results.push(pct);
                if cli.quiet {
                    println!("{pct:.6}%");
                }
            }
            if cli.n_times > 1 {
                crate::report::multi_run::report_stats(&results);
            }
        }
        crate::config::Mode::Optimize => {
            let opt = cfg
                .optimizer
                .ok_or_else(|| anyhow::anyhow!("Missing [optimizer] config"))?;
            let mut results = Vec::with_capacity(cli.n_times);
            for _ in 0..cli.n_times {
                let pct = crate::modes::optimize::run_optimize(
                    &opt,
                    &cfg.data,
                    cli.quiet,
                )?;
                results.push(pct);
                if cli.quiet {
                    println!("{pct:.6}%");
                }
            }
            if cli.n_times > 1 {
                crate::report::multi_run::report_stats(&results);
            }
        }
    }

    Ok(())
}

fn init_tracing(quiet: bool) {
    if quiet {
        return;
    }
    use tracing_subscriber::EnvFilter;
    use std::io::IsTerminal;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let ansi = std::io::stdout().is_terminal();
    tracing_subscriber::fmt()
        .compact()
        .without_time()
        .with_env_filter(filter)
        .with_ansi(ansi)
        .with_target(false)
        .with_writer(std::io::stdout)
        .init();
}

#[derive(Debug)]
struct Cli {
    config_path: std::path::PathBuf,
    quiet: bool,
    n_times: usize,
}

fn parse_args() -> anyhow::Result<Cli> {
    let mut args = std::env::args().skip(1);
    let mut cfg: Option<std::path::PathBuf> = None;
    let mut quiet = false;
    let mut n_times: usize = 1;

    while let Some(a) = args.next() {
        // Accept `backster config <file.toml>` as a convenience.
        if a == "config" && cfg.is_none() {
            cfg = args.next().map(std::path::PathBuf::from);
            continue;
        }
        if a == "--quiet" || a == "-q" {
            quiet = true;
            continue;
        }
        if a == "--n-times" {
            let Some(v) = args.next() else {
                anyhow::bail!("--n-times requires a value");
            };
            n_times = v
                .parse::<usize>()
                .map_err(|_| anyhow::anyhow!("Invalid --n-times value: {v:?}"))?;
            continue;
        }
        if let Some(v) = a.strip_prefix("--n-times=") {
            n_times = v
                .parse::<usize>()
                .map_err(|_| anyhow::anyhow!("Invalid --n-times value: {v:?}"))?;
            continue;
        }
        if a == "--config" || a == "-c" {
            cfg = args.next().map(std::path::PathBuf::from);
        } else if cfg.is_none() {
            cfg = Some(std::path::PathBuf::from(a));
        }
    }

    let config_path = cfg.ok_or_else(|| {
        anyhow::anyhow!(
            "Usage: backster [--quiet|-q] [--n-times N] --config <config.toml>"
        )
    })?;
    anyhow::ensure!(n_times >= 1, "--n-times must be >= 1");
    Ok(Cli {
        config_path,
        quiet,
        n_times,
    })
}
