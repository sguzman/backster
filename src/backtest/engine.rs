use anyhow::Result;
use chrono::{DateTime, Utc};
use rand::SeedableRng;
use tracing::info;

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
    allow_margin: bool,
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
            allow_margin: false,
            rng: rand_chacha::ChaCha8Rng::seed_from_u64(seed),
        }
    }

    pub fn set_logging(&mut self, log_trades: bool, log_strategy: bool, trade_resolution: String, allow_margin: bool) {
        self.log_trades = log_trades;
        self.log_strategy = log_strategy;
        self.trade_resolution = trade_resolution;
        self.allow_margin = allow_margin;
    }

    pub fn rng(&mut self) -> &mut rand_chacha::ChaCha8Rng {
        &mut self.rng
    }

    pub fn log_strategy(&self) -> bool {
        self.log_strategy
    }

    pub fn allow_margin(&self) -> bool {
        self.allow_margin
    }

    pub fn enter_long(&mut self, qty: f64, price: f64, ts: DateTime<Utc>) -> Result<()> {
        anyhow::ensure!(self.position.is_none(), "Already in a position");
        anyhow::ensure!(qty.is_finite() && qty > 0.0, "Invalid qty");
        anyhow::ensure!(price.is_finite() && price > 0.0, "Invalid price");
        let cost = qty * price;
        if !self.allow_margin {
            anyhow::ensure!(self.cash + 1e-9 >= cost, "Insufficient cash");
        }
        self.cash -= cost;
        self.position = Some(Position {
            side: Side::Long,
            qty,
            entry_price: price,
            entry_ts: ts,
        });
        if self.log_trades {
            info!(
                ts = %ts.to_rfc3339(),
                res = %self.trade_resolution,
                side = "long",
                qty = qty,
                price = price,
                cash = self.cash,
                "trade_enter"
            );
        }
        Ok(())
    }

    pub fn enter_short(&mut self, qty: f64, price: f64, ts: DateTime<Utc>) -> Result<()> {
        anyhow::ensure!(self.position.is_none(), "Already in a position");
        anyhow::ensure!(qty.is_finite() && qty > 0.0, "Invalid qty");
        anyhow::ensure!(price.is_finite() && price > 0.0, "Invalid price");
        let collateral = qty * price;
        if !self.allow_margin {
            anyhow::ensure!(self.cash + 1e-9 >= collateral, "Insufficient cash (short collateral)");
        }
        self.cash -= collateral;
        self.position = Some(Position {
            side: Side::Short,
            qty,
            entry_price: price,
            entry_ts: ts,
        });
        if self.log_trades {
            info!(
                ts = %ts.to_rfc3339(),
                res = %self.trade_resolution,
                side = "short",
                qty = qty,
                price = price,
                cash = self.cash,
                "trade_enter"
            );
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
            info!(
                ts = %ts.to_rfc3339(),
                res = %self.trade_resolution,
                side = ?pos.side,
                entry = pos.entry_price,
                exit = price,
                qty = pos.qty,
                pnl = pnl,
                cash = self.cash,
                "trade_exit"
            );
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

    pub fn run(&self, bars: &[Bar], strategy: &mut dyn Strategy) -> Result<BacktestContext> {
        let seed = {
            use rand::TryRngCore;
            let mut bytes = [0u8; 8];
            let mut rng = rand::rngs::OsRng;
            rng.try_fill_bytes(&mut bytes).expect("OsRng failed");
            u64::from_le_bytes(bytes)
        };
        strategy.on_data(bars)?;
        let mut ctx = BacktestContext::new(self.starting_cash, seed);
        ctx.set_logging(
            self.log.log_trades,
            self.log.log_strategy,
            self.log.trade_resolution.clone(),
            self.log.allow_margin,
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
                info!(
                    ts = %bar.ts.to_rfc3339(),
                    trade_res = %self.log.trade_resolution,
                    close = bar.close,
                    cash_before = before_cash,
                    cash_after = ctx.cash,
                    eq_before = before_equity,
                    eq_after = ctx.equity(bar.close),
                    position = %pos_desc,
                    "bar"
                );
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
    pub allow_margin: bool,
}
