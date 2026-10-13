use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub name: String,
    pub root: String,
    pub executable: String,
    pub enabled: bool,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    #[serde(flatten)]
    pub source: Source,
    pub status: String,
    pub last_scan: Option<String>,
    pub last_activity: Option<String>,
    pub files: usize,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cached: u64,
    pub cache_write: u64,
    pub reasoning: Option<u64>,
    pub calls: Option<u64>,
    pub cost: Option<f64>,
}

impl Usage {
    pub fn total(&self) -> u64 {
        self.input.saturating_add(self.output)
    }
    pub fn add(&mut self, other: &Self) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.cached = self.cached.saturating_add(other.cached);
        self.cache_write = self.cache_write.saturating_add(other.cache_write);
        self.reasoning = sum_optional(self.reasoning, other.reasoning);
        self.calls = sum_optional(self.calls, other.calls);
        self.cost = match (self.cost, other.cost) {
            (Some(a), Some(b)) => Some(a + b),
            _ => None,
        };
    }
}
fn sum_optional(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.saturating_add(b)),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub id: String,
    pub session_id: String,
    pub agent: String,
    pub project: String,
    pub model: String,
    pub day: String,
    pub timestamp: String,
    pub started_at: String,
    pub source_file: String,
    pub parent_id: Option<String>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchEntry {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub target: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Script {
    pub id: String,
    pub name: String,
    pub path: String,
    pub interpreter: String,
    pub cwd: String,
    pub args: Vec<String>,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub theme: String,
    pub refresh_seconds: u64,
    pub close_to_tray: bool,
    pub sources: Vec<Source>,
    pub entries: Vec<LaunchEntry>,
    pub scripts: Vec<Script>,
    #[serde(default)]
    pub skills: crate::skills::SkillsSettings,
}

impl Default for Settings {
    fn default() -> Self {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_default();
        let root = std::path::Path::new(&home);
        let sources = [
            ("codex", "Codex", ".codex"),
            ("claude", "Claude Code", ".claude"),
            ("dsh", "DeepSeek Harness", ".dsh"),
        ]
        .iter()
        .map(|(id, name, folder)| Source {
            id: id.to_string(),
            name: name.to_string(),
            root: root.join(folder).to_string_lossy().into_owned(),
            executable: find_executable(id).unwrap_or_default(),
            enabled: true,
            version: None,
        })
        .collect();
        Self {
            theme: "system".into(),
            refresh_seconds: 60,
            close_to_tray: true,
            sources,
            entries: vec![],
            scripts: vec![],
            skills: crate::skills::SkillsSettings::default(),
        }
    }
}

pub fn find_executable(name: &str) -> Option<String> {
    for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        for extension in [".exe", ".ps1", ""] {
            let p = directory.join(format!("{name}{extension}"));
            if p.is_file() {
                return Some(p.to_string_lossy().into_owned());
            }
        }
    }
    let appdata = std::env::var_os("APPDATA")?;
    let p = std::path::PathBuf::from(appdata)
        .join("npm")
        .join(format!("{name}.ps1"));
    p.is_file().then(|| p.to_string_lossy().into_owned())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub sources: Vec<SourceStatus>,
    pub records: Vec<Record>,
    pub collected_at: String,
    pub data_directory: String,
}
