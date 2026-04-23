use anyhow::Result;

pub struct OptimizeInput<'a> {
    pub prices: &'a [f64],
    pub starting_cash: f64,
    pub max_trades: usize,
    pub allow_long: bool,
    pub allow_short: bool,
}

pub struct OptimizeResult {
    pub final_cash: f64,
    pub trades_used: usize,
}

pub fn optimize_trades(input: OptimizeInput<'_>) -> Result<OptimizeResult> {
    anyhow::ensure!(input.starting_cash.is_finite() && input.starting_cash > 0.0, "Bad cash");
    anyhow::ensure!(input.max_trades > 0, "max_trades must be > 0");
    anyhow::ensure!(
        input.allow_long || input.allow_short,
        "Must allow long and/or short"
    );
    anyhow::ensure!(input.prices.len() >= 2, "Need at least 2 prices");

    // Hindsight optimizer (know_future=true): maximize final cash under a round-trip trade limit.
    // Uses a log-wealth DP where each close (exit) increments `trades_used` by 1.
    let k = input.max_trades;

    let mut flat = vec![f64::NEG_INFINITY; k + 1];
    let mut long = vec![f64::NEG_INFINITY; k + 1];
    let mut short = vec![f64::NEG_INFINITY; k + 1];
    flat[0] = input.starting_cash.ln();

    for &price in input.prices {
        anyhow::ensure!(price.is_finite() && price > 0.0, "Bad price in series");
        let lp = price.ln();

        for t in 0..=k {
            if input.allow_long {
                long[t] = long[t].max(flat[t] - lp); // open long
            }
            if input.allow_short {
                short[t] = short[t].max(flat[t] + lp); // open short (profit factor entry/exit)
            }
        }

        for t in 0..k {
            if input.allow_long {
                flat[t + 1] = flat[t + 1].max(long[t] + lp); // close long
            }
            if input.allow_short {
                flat[t + 1] = flat[t + 1].max(short[t] - lp); // close short
            }
        }
    }

    let (best_t, best_log_cash) = flat
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Less))
        .map(|(t, &v)| (t, v))
        .unwrap();

    Ok(OptimizeResult {
        final_cash: best_log_cash.exp(),
        trades_used: best_t,
    })
}
