use anyhow::{Context, Result};
use std::process::Command;
use std::path::{Path, PathBuf};
use polars::prelude::*;

pub struct WolframRunner {
    kernel_path: PathBuf,
}

impl WolframRunner {
    pub fn new<P: Into<PathBuf>>(kernel_path: P) -> Self {
        Self {
            kernel_path: kernel_path.into(),
        }
    }

    pub fn run_script<P: AsRef<Path>>(&self, script_path: P, data_path: P) -> Result<()> {
        let output = Command::new(&self.kernel_path)
            .arg("-script")
            .arg(script_path.as_ref())
            .arg(data_path.as_ref())
            .output()
            .context("Failed to execute Wolfram script")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Wolfram script failed: {}", stderr);
        }

        Ok(())
    }
}
