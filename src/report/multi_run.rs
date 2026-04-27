use tracing::info;

pub fn report_stats(results: &[f64]) {
    if results.is_empty() {
        return;
    }

    let n = results.len() as f64;
    let mut sorted = results.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let min = sorted[0];
    let max = sorted[sorted.len() - 1];
    let mean = results.iter().sum::<f64>() / n;

    let median = if results.len() % 2 == 0 {
        (sorted[results.len() / 2 - 1] + sorted[results.len() / 2]) / 2.0
    } else {
        sorted[results.len() / 2]
    };

    let variance = results.iter().map(|x| {
        let diff = x - mean;
        diff * diff
    }).sum::<f64>() / (n - 1.0);
    let std_dev = variance.sqrt();

    // Skewness and Kurtosis (Excess)
    let mut m3 = 0.0;
    let mut m4 = 0.0;
    if std_dev > 0.0 {
        for x in results {
            let z = (x - mean) / std_dev;
            m3 += z.powi(3);
            m4 += z.powi(4);
        }
        m3 /= n;
        m4 /= n;
    }
    let skewness = m3;
    let kurtosis = m4 - 3.0;

    // Geometric Mean of (1 + r/100)
    // We assume results are percentage returns (e.g. 5.0 for 5%)
    let mut geo_mean = 0.0;
    let mut possible = true;
    let mut log_sum = 0.0;
    for x in results {
        let val = 1.0 + x / 100.0;
        if val <= 0.0 {
            possible = false;
            break;
        }
        log_sum += val.ln();
    }
    if possible {
        geo_mean = ((log_sum / n).exp() - 1.0) * 100.0;
    }

    println!("--- Multi-Run Statistics (n={}) ---", results.len());
    println!("Min:            {:>10.4}%", min);
    println!("Max:            {:>10.4}%", max);
    println!("Mean:           {:>10.4}%", mean);
    println!("Median:         {:>10.4}%", median);
    println!("Std Dev:        {:>10.4}%", std_dev);
    println!("Skewness:       {:>10.4}", skewness);
    println!("Kurtosis:       {:>10.4}", kurtosis);
    if possible {
        println!("Geo Mean:       {:>10.4}%", geo_mean);
    } else {
        println!("Geo Mean:       {:>10}", "N/A (contains total loss)");
    }
}
