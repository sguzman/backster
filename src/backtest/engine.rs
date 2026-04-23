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
}

impl BacktestContext {
    pub fn new(starting_cash: f64) -> Self {
        Self {
            cash: starting_cash,
            position: None,
            stats: BacktestStats::default(),
        }
    }

    pub fn enter_long(&mut self, qty: f64, price: f64, ts: DateTime<Utc>) -> Result<()> {
        anyhow::ensure!(self.position.is_none(), "Already in a position");
        self.position = Some(Position {
            side: Side::Long,
            qty,
            entry_price: price,
            entry_ts: ts,
        });
        Ok(())
    }

    pub fn exit(&mut self, price: f64, ts: DateTime<Utc>) -> Result<()> {
        let pos = self.position.take().ok_or_else(|| anyhow::anyhow!("No open position"))?;
        let pnl = match pos.side {
            Side::Long => (price - pos.entry_price) * pos.qty,
            Side::Short => (pos.entry_price - price) * pos.qty,
        };
        self.cash += pnl;
        self.stats.realized_pnl += pnl;
        self.stats.trades += 1;
        let _ = ts;
        Ok(())
    }
}

pub struct BacktestEngine {
    starting_cash: f64,
}

impl BacktestEngine {
    pub fn new(starting_cash: f64) -> Self {
        Self { starting_cash }
    }

    pub fn run(&self, bars: &[Bar], strategy: &mut dyn Strategy) -> Result<BacktestContext> {
        let mut ctx = BacktestContext::new(self.starting_cash);
        strategy.on_start(&mut ctx)?;
        for bar in bars {
            strategy.on_bar(&mut ctx, bar)?;
        }
        strategy.on_finish(&mut ctx)?;
        Ok(ctx)
    }
}

