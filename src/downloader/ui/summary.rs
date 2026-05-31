use anyhow::Result;

pub struct DownloadSummary {
    pub downloaded: usize,
    pub skipped: usize,
    pub failed: Vec<(String, anyhow::Error)>,
}

impl DownloadSummary {
    pub fn new() -> Self {
        Self {
            downloaded: 0,
            skipped: 0,
            failed: Vec::new(),
        }
    }

    pub fn from_results(results: Vec<(String, Result<bool>)>) -> Self {
        let mut summary = Self::new();
        for (track_name, result) in results {
            match result {
                Ok(true) => summary.downloaded += 1,
                Ok(false) => summary.skipped += 1,
                Err(e) => summary.failed.push((track_name, e)),
            }
        }
        summary
    }

    pub fn print(&self) {
        println!("\nsummary:");
        println!("  downloaded: {}", self.downloaded);
        if self.skipped > 0 {
            println!("  skipped: {} (already exist)", self.skipped);
        }
        if !self.failed.is_empty() {
            println!("  failed: {}", self.failed.len());
            for track in &self.failed {
                println!("    - {} ({})", track.0, track.1.to_string());
            }
        }
    }
}
