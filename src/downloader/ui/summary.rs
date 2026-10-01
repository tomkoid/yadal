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

    pub fn merge(&mut self, other: DownloadSummary) {
        self.downloaded += other.downloaded;
        self.skipped += other.skipped;

        self.failed.extend(other.failed);
    }

    pub fn print(&self) {
        println!("summary:");
        println!("  downloaded: {}", self.downloaded);
        if self.skipped > 0 {
            println!("  skipped: {} (already exist)", self.skipped);
        }
        if !self.failed.is_empty() {
            println!("  failed: {}", self.failed.len());
            for track in &self.failed {
                println!("{}- {} ({})", " ".repeat(4), track.0, track.1);
            }
        }
    }

    pub fn did_fail(&self) -> bool {
        !self.failed.is_empty()
    }
}
