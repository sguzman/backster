use anyhow::Result;
use chrono::{DateTime, Utc};

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
    log_trades: bool,
    trade_resolution: String,
}

impl BacktestContext {
    pub fn new(starting_cash: f64) -> Self {
        Self {
            cash: starting_cash,
            position: None,
            stats: BacktestStats::default(),
            log_trades: false,
            trade_resolution: "bar".to_string(),
        }
    }

    pub fn set_trade_logging(&mut self, enabled: bool, trade_resolution: String) {
        self.log_trades = enabled;
        self.trade_resolution = trade_resolution;
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
            eprintln!(
                "[trade][enter][{}][res={}] qty={:.6} price={:.6} cash={:.2}",
                ts.to_rfc3339(),
                self.trade_resolution,
                qty,
                price,
                self.cash
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
            eprintln!(
                "[trade][exit][{}][res={}] entry={:.6} exit={:.6} qty={:.6} pnl={:.6} cash={:.2}",
                ts.to_rfc3339(),
                self.trade_resolution,
                pos.entry_price,
                price,
                pos.qty,
                pnl,
                self.cash
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
        let mut ctx = BacktestContext::new(self.starting_cash);
        ctx.set_trade_logging(self.log.log_trades, self.log.trade_resolution.clone());
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
                eprintln!(
                    "[bar][{}][trade_res={}]\tclose={:.6}\tcash={:.2}->{:.2}\teq={:.2}->{:.2}\t{}",
                    bar.ts.to_rfc3339(),
                    self.log.trade_resolution,
                    bar.close,
                    before_cash,
                    ctx.cash,
                    before_equity,
                    ctx.equity(bar.close),
                    pos_desc
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
    pub trade_resolution: String,
}
