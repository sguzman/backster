use anyhow::Result;
use chrono::{DateTime, Utc};
use rand::SeedableRng;
use tracing::info;
use owo_colors::OwoColorize;

use crate::strategy::Strategy;

#[derive(Debug, Clone)]
pub struct Bar {
    pub ts: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Long,
    Short,
}

#[derive(Debug, Clone)]
pub struct Position {
    pub side: Side,
    pub qty: f64,
    pub entry_price: f64,
    pub entry_ts: DateTime<Utc>,
}

#[derive(Debug, Default, Clone)]
pub struct BacktestStats {
    pub realized_pnl: f64,
    pub trades: u64,
}

pub struct BacktestContext {
    pub cash: f64,
    pub position: Option<Position>,
    pub stats: BacktestStats,
    pub seed: u64,
    log_trades: bool,
    log_strategy: bool,
    trade_resolution: String,
    rng: rand_chacha::ChaCha8Rng,
}

impl BacktestContext {
    pub fn new(starting_cash: f64, seed: u64) -> Self {
        Self {
            cash: starting_cash,
            position: None,
            stats: BacktestStats::default(),
            seed,
            log_trades: false,
            log_strategy: false,
            trade_resolution: "bar".to_string(),
            rng: rand_chacha::ChaCha8Rng::seed_from_u64(seed),
        }
    }

    pub fn set_logging(&mut self, log_trades: bool, log_strategy: bool, trade_resolution: String) {
        self.log_trades = log_trades;
        self.log_strategy = log_strategy;
        self.trade_resolution = trade_resolution;
    }

    pub fn rng(&mut self) -> &mut rand_chacha::ChaCha8Rng {
        &mut self.rng
    }

    pub fn log_strategy(&self) -> bool {
        self.log_strategy
    }

    pub fn enter_long(&mut self, qty: f64, price: f64, ts: DateTime<Utc>) -> Result<()> {
        anyhow::ensure!(self.position.is_none(), "Already in a position");
        anyhow::ensure!(qty.is_finite() && qty > 0.0, "Invalid qty");
        anyhow::ensure!(price.is_finite() && price > 0.0, "Invalid price");
        let cost = qty * price;
        anyhow::ensure!(self.cash + 1e-9 >= cost, "Insufficient cash");
        self.cash -= cost;
        self.position = Some(Position {
            side: Side::Long,
            qty,
            entry_price: price,
            entry_ts: ts,
        });
        if self.log_trades {
            let msg = format!(
                "{} {} res={} qty={:.6} price={:.6} cash={:.2}",
                "[trade]".bright_black(),
                "ENTER_LONG".green().bold(),
                self.trade_resolution.cyan(),
                qty,
                price,
                self.cash
            );
            info!(ts = %ts.to_rfc3339(), "{msg}");
        }
        Ok(())
    }

    pub fn enter_short(&mut self, qty: f64, price: f64, ts: DateTime<Utc>) -> Result<()> {
        anyhow::ensure!(self.position.is_none(), "Already in a position");
        anyhow::ensure!(qty.is_finite() && qty > 0.0, "Invalid qty");
        anyhow::ensure!(price.is_finite() && price > 0.0, "Invalid price");
        let collateral = qty * price;
        anyhow::ensure!(self.cash + 1e-9 >= collateral, "Insufficient cash (short collateral)");
        self.cash -= collateral;
        self.position = Some(Position {
            side: Side::Short,
            qty,
            entry_price: price,
            entry_ts: ts,
        });
        if self.log_trades {
            let msg = format!(
                "{} {} res={} qty={:.6} price={:.6} cash={:.2}",
                "[trade]".bright_black(),
                "ENTER_SHORT".red().bold(),
                self.trade_resolution.cyan(),
                qty,
                price,
                self.cash
            );
            info!(ts = %ts.to_rfc3339(), "{msg}");
        }
        Ok(())
    }

    pub fn exit(&mut self, price: f64, ts: DateTime<Utc>) -> Result<()> {
        let pos = self.position.take().ok_or_else(|| anyhow::anyhow!("No open position"))?;
        anyhow::ensure!(price.is_finite() && price > 0.0, "Invalid price");
        let pnl = match pos.side {
            Side::Long => (price - pos.entry_price) * pos.qty,
            Side::Short => (pos.entry_price - price) * pos.qty,
        };
        let proceeds = match pos.side {
            Side::Long => pos.qty * price,
            Side::Short => pos.qty * (2.0 * pos.entry_price - price),
        };
        self.cash += proceeds;
        self.stats.realized_pnl += pnl;
        self.stats.trades += 1;
        if self.log_trades {
            let kind: String = match pos.side {
                Side::Long => "EXIT_LONG".green().bold().to_string(),
                Side::Short => "EXIT_SHORT".red().bold().to_string(),
            };
            let pnl_s: String = if pnl >= 0.0 {
                format!("{pnl:.6}").green().to_string()
            } else {
                format!("{pnl:.6}").red().to_string()
            };
            let msg = format!(
                "{} {} res={} entry={:.6} exit={:.6} qty={:.6} pnl={} cash={:.2}",
                "[trade]".bright_black(),
                kind,
                self.trade_resolution.cyan(),
                pos.entry_price,
                price,
                pos.qty,
                pnl_s,
                self.cash
            );
            info!(ts = %ts.to_rfc3339(), "{msg}");
        }
        let _ = ts;
        Ok(())
    }

    pub fn equity(&self, mark_price: f64) -> f64 {
        if let Some(pos) = &self.position {
            match pos.side {
                Side::Long => self.cash + pos.qty * mark_price,
                Side::Short => self.cash + pos.qty * (2.0 * pos.entry_price - mark_price),
            }
        } else {
            self.cash
        }
    }
}

pub struct BacktestEngine {
    starting_cash: f64,
    log: BacktestLogConfig,
}

impl BacktestEngine {
    pub fn new(starting_cash: f64, log: BacktestLogConfig) -> Self {
        Self { starting_cash, log }
    }

    pub fn run(
        &self,
        bars: &[Bar],
        strategy: &mut dyn Strategy,
        seed: Option<u64>,
    ) -> Result<BacktestContext> {
        let seed = seed.unwrap_or_else(|| {
            use rand::TryRngCore;
            let mut bytes = [0u8; 8];
            let mut rng = rand::rngs::OsRng;
            rng.try_fill_bytes(&mut bytes).expect("OsRng failed");
            u64::from_le_bytes(bytes)
        });
        let mut ctx = BacktestContext::new(self.starting_cash, seed);
        ctx.set_logging(
            self.log.log_trades,
            self.log.log_strategy,
            self.log.trade_resolution.clone(),
        );
        strategy.on_start(&mut ctx)?;

        for bar in bars {
            let before_cash = ctx.cash;
            let before_equity = ctx.equity(bar.close);
            let _before_pos = ctx.position.clone();

            strategy.on_bar(&mut ctx, bar)?;
            let _ = _before_pos;

            if self.log.log_bars {
                let pos_desc = ctx
                    .position
                    .as_ref()
                    .map(|p| format!("{:?} qty={:.6} entry={:.6}", p.side, p.qty, p.entry_price))
                    .unwrap_or_else(|| "flat".to_string());
                let msg = format!(
                    "{} res={} close={} cash={}→{} eq={}→{} {}",
                    "[bar]".bright_black(),
                    self.log.trade_resolution.cyan(),
                    format!("{:.6}", bar.close).white().bold(),
                    format!("{before_cash:.2}").yellow(),
                    format!("{:.2}", ctx.cash).yellow(),
                    format!("{before_equity:.2}").magenta(),
                    format!("{:.2}", ctx.equity(bar.close)).magenta(),
                    pos_desc.bright_black(),
                );
                info!(ts = %bar.ts.to_rfc3339(), "{msg}");
            }

        }
        strategy.on_finish(&mut ctx)?;
        Ok(ctx)
    }
}

#[derive(Debug, Clone)]
pub struct BacktestLogConfig {
    pub log_bars: bool,
    pub log_trades: bool,
    pub log_strategy: bool,
    pub trade_resolution: String,
}
