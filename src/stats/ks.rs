use anyhow::Result;

/// Two-sided Kolmogorov–Smirnov statistic + asymptotic p-value approximation.
///
/// Note: This is the classic KS p-value for a fully-specified distribution. When
/// distribution parameters are estimated from the same sample, the p-values are
/// optimistic (i.e. too large). For research/prototyping this is often acceptable,
/// but treat it as a heuristic rather than a strict hypothesis test.
pub fn ks_p_value(sample: &[f64], cdf: impl Fn(f64) -> f64) -> Result<(f64, f64)> {
    anyhow::ensure!(!sample.is_empty(), "Empty sample");

    let mut xs: Vec<f64> = sample.iter().copied().filter(|x| x.is_finite()).collect();
    anyhow::ensure!(!xs.is_empty(), "Sample has no finite values");
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let n = xs.len() as f64;
    let mut d: f64 = 0.0;

    for (i, &x) in xs.iter().enumerate() {
        let f = cdf(x).clamp(0.0, 1.0);
        let i0 = i as f64 / n;
        let i1 = (i as f64 + 1.0) / n;
        d = d.max((f - i0).abs()).max((i1 - f).abs());
    }

    let p = ks_asymptotic_p_value(d, xs.len());
    Ok((d, p))
}

fn ks_asymptotic_p_value(d: f64, n: usize) -> f64 {
    if !(d.is_finite()) || d < 0.0 {
        return 0.0;
    }
    if d == 0.0 {
        return 1.0;
    }
    let n = n as f64;
    if n <= 0.0 {
        return 0.0;
    }

    // Stephens' correction often used for better small-n approximation.
    let en = (n).sqrt();
    let lambda = (en + 0.12 + 0.11 / en) * d;

    // Q_KS(lambda) = 2 * sum_{j=1..inf} (-1)^{j-1} exp(-2 j^2 lambda^2)
    let mut sum = 0.0;
    for j in 1..=200 {
        let jj = j as f64;
        let term = (-2.0 * jj * jj * lambda * lambda).exp();
        let signed = if j % 2 == 1 { term } else { -term };
        sum += signed;
        if term < 1e-12 {
            break;
        }
    }
    (2.0 * sum).clamp(0.0, 1.0)
}
