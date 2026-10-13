use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use uuid::Uuid;
use walkdir::WalkDir;

mod batch;
pub use batch::*;

type Result<T> = std::result::Result<T, String>;
fn now() -> String {
    Utc::now().to_rfc3339()
}
fn display(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}
fn io<T>(r: std::io::Result<T>) -> Result<T> {
    r.map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationSettings {
    pub source_path: String,
    pub status: String,
    pub backup_path: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsSettings {
    pub central_path: String,
    pub default_sync_mode: String,
    pub repositories: Vec<String>,
    pub projects: Vec<String>,
    #[serde(default)]
    pub auto_check_updates: bool,
    #[serde(default)]
    pub allow_copy: bool,
    pub migration: MigrationSettings,
}
impl Default for SkillsSettings {
    fn default() -> Self {
        Self {
            central_path: "~/.agents/skills".into(),
            default_sync_mode: "auto".into(),
            repositories: vec![],
            projects: vec![],
            auto_check_updates: false,
            allow_copy: false,
            migration: MigrationSettings {
                source_path: "~/.skills-manager/skills".into(),
                status: "pending".into(),
                backup_path: None,
            },
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub central_path: String,
    pub source_type: String,
    pub source_url: Option<String>,
    pub source_subpath: Option<String>,
    pub revision: Option<String>,
    pub content_hash: String,
    pub updated_at: String,
    pub file_count: usize,
    pub document: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillTarget {
    pub id: String,
    pub skill_id: String,
    pub agent_id: String,
    pub scope: String,
    pub project_path: Option<String>,
    pub target_path: String,
    pub mode: String,
    pub status: String,
    pub last_synced_at: Option<String>,
    pub source_hash: String,
    pub error: Option<String>,
    pub managed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub id: String,
    pub url: String,
    pub reference: String,
    pub checkout_path: String,
    pub revision: String,
    pub last_checked_at: String,
    pub error: Option<String>,
    pub update_available: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub path: String,
    pub name: String,
    pub description: String,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub skills: Vec<Skill>,
    pub diagnostics: Vec<String>,
    pub central_path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRequest {
    pub skill_id: String,
    pub agent_id: String,
    pub project_path: Option<String>,
    pub mode: String,
    pub allow_copy: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDifference {
    pub path: String,
    pub status: String,
    pub central: Option<String>,
    pub target: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPreview {
    pub id: String,
    pub skill_id: String,
    pub source_path: String,
    pub target_path: String,
    pub mode: String,
    pub status: String,
    pub conflict: bool,
    pub managed: bool,
    pub differences: Vec<FileDifference>,
    pub target_document: Option<String>,
}
struct SavedSync {
    request: SyncRequest,
    preview: SyncPreview,
    source_hash: String,
    target_fingerprint: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationRow {
    pub name: String,
    pub source_path: String,
    pub target_path: String,
    pub status: String,
    pub links: Vec<String>,
    pub differences: Vec<FileDifference>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPreview {
    pub id: String,
    pub source_path: String,
    pub central_path: String,
    pub rows: Vec<MigrationRow>,
    pub diagnostics: Vec<String>,
    pub directory_links: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolution {
    pub name: String,
    pub choice: String,
}
struct SavedMigration {
    preview: MigrationPreview,
    fingerprints: Vec<(String, String, Vec<String>)>,
    roots: Vec<(PathBuf, String)>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    pub message: String,
    pub backup_path: Option<String>,
}

pub struct SkillsService {
    data: PathBuf,
    home: PathBuf,
    central: PathBuf,
    conn: Connection,
    sync_plans: HashMap<String, SavedSync>,
    migration_plans: HashMap<String, SavedMigration>,
    batch_plans: HashMap<String, batch::SavedBatch>,
}

pub fn validate_name(name: &str) -> Result<()> {
    let upper = name.to_ascii_uppercase();
    if name.is_empty()
        || name.len() > 100
        || name.starts_with('.')
        || name.ends_with('.')
        || name.ends_with(' ')
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        || [
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ]
        .contains(&upper.as_str())
        || ["synced", "anthropic-skills"].contains(&name.to_ascii_lowercase().as_str())
    {
        return Err(format!("非法或保留的 Skill 名称：{name}"));
    }
    Ok(())
}
fn document_path(path: &Path) -> Result<PathBuf> {
    ["SKILL.md", "skill.md"]
        .iter()
        .map(|n| path.join(n))
        .find(|p| p.is_file())
        .ok_or_else(|| format!("缺少 SKILL.md：{}", display(path)))
}
fn metadata(document: &str) -> Result<(String, String)> {
    let text = document
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n");
    let mut lines = text.lines();
    if lines.next() != Some("---") {
        return Err("SKILL.md 缺少 YAML frontmatter".into());
    }
    let mut yaml = String::new();
    let mut closed = false;
    for line in lines {
        if line == "---" {
            closed = true;
            break;
        }
        yaml.push_str(line);
        yaml.push('\n');
    }
    if !closed {
        return Err("frontmatter 未闭合".into());
    }
    let fields: serde_yaml::Value =
        serde_yaml::from_str(&yaml).map_err(|e| format!("frontmatter 无效：{e}"))?;
    let name = fields["name"]
        .as_str()
        .ok_or("frontmatter 缺少字符串 name")?
        .trim()
        .to_string();
    let description = fields["description"]
        .as_str()
        .ok_or("frontmatter 缺少字符串 description")?
        .trim()
        .to_string();
    validate_name(&name)?;
    if description.is_empty() {
        return Err("description 不能为空".into());
    }
    Ok((name, description))
}
fn is_link(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        fs::symlink_metadata(path).is_ok_and(|m| m.file_attributes() & 0x400 != 0)
    }
    #[cfg(not(windows))]
    {
        fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
    }
}
fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}
// 每个支持文件都参与哈希；不跟随内部链接，避免导入库外内容或循环。
fn files(path: &Path) -> Result<BTreeMap<String, PathBuf>> {
    if !path.is_dir() {
        return Err(format!("目录不可读：{}", display(path)));
    }
    let mut result = BTreeMap::new();
    for item in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.file_name() != ".git")
    {
        let item = item.map_err(|e| e.to_string())?;
        if item.depth() == 0 {
            continue;
        }
        if is_link(item.path()) {
            return Err(format!(
                "Skill 内部链接不允许导入：{}",
                display(item.path())
            ));
        }
        if item.file_type().is_file() {
            result.insert(
                display(io(item
                    .path()
                    .strip_prefix(path)
                    .map_err(std::io::Error::other))?)
                .replace('\\', "/"),
                item.into_path(),
            );
        }
    }
    Ok(result)
}
fn hash_directory(path: &Path) -> Result<(String, usize)> {
    let entries = files(path)?;
    let mut hash = Sha256::new();
    for (name, file) in &entries {
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update(io(fs::metadata(file))?.len().to_le_bytes());
        let mut f = io(fs::File::open(file))?;
        let mut buffer = [0; 65536];
        loop {
            let n = io(f.read(&mut buffer))?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
    }
    Ok((format!("{:x}", hash.finalize()), entries.len()))
}
fn hash_file(path: &Path) -> Result<String> {
    let mut file = io(fs::File::open(path))?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = io(file.read(&mut buffer))?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn read_skill(path: &Path) -> Result<Skill> {
    let doc = document_path(path)?;
    if io(fs::metadata(&doc))?.len() > 1024 * 1024 {
        return Err("SKILL.md 超过 1 MiB".into());
    }
    let document = io(fs::read_to_string(doc))?;
    let (name, description) = metadata(&document)?;
    let (content_hash, file_count) = hash_directory(path)?;
    Ok(Skill {
        id: name.clone(),
        name,
        description,
        central_path: display(path),
        source_type: "local".into(),
        source_url: None,
        source_subpath: None,
        revision: None,
        content_hash,
        updated_at: now(),
        file_count,
        document,
    })
}
fn absolute(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err("必须使用绝对路径".into());
    }
    let mut clean = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                if !clean.pop() {
                    return Err("路径越界".into());
                }
            }
            Component::CurDir => {}
            _ => clean.push(part.as_os_str()),
        }
    }
    Ok(clean)
}
fn resolved(path: &Path) -> Result<PathBuf> {
    let p = absolute(path)?;
    if exists(&p) {
        return io(fs::canonicalize(p));
    }
    let parent = p.parent().ok_or("路径没有父目录")?;
    Ok(resolved(parent)?.join(p.file_name().ok_or("路径没有文件名")?))
}
fn key(path: &Path) -> String {
    let s = display(path)
        .trim_start_matches("\\\\?\\")
        .replace('\\', "/");
    #[cfg(windows)]
    {
        s.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        s
    }
}
fn same(a: &Path, b: &Path) -> Result<bool> {
    Ok(key(&resolved(a)?) == key(&resolved(b)?))
}
fn nested(a: &Path, b: &Path) -> Result<bool> {
    let a = key(&resolved(a)?);
    let b = key(&resolved(b)?);
    Ok(a == b || a.starts_with(&(b.clone() + "/")) || b.starts_with(&(a + "/")))
}
fn fingerprint(path: &Path) -> Result<String> {
    if !exists(path) {
        return Ok("missing".into());
    }
    let prefix = if is_link(path) {
        format!("link:{}:", display(&io(fs::read_link(path))?))
    } else {
        "directory:".into()
    };
    if !path.is_dir() {
        return Ok(format!("{prefix}file:{:x}", Sha256::digest(io(fs::read(path))?)));
    }
    Ok(prefix + &hash_directory(path)?.0)
}
fn copy_directory(source: &Path, target: &Path) -> Result<()> {
    if exists(target) {
        return Err(format!("目标已存在：{}", display(target)));
    }
    io(fs::create_dir_all(target))?;
    for (name, path) in files(source)? {
        let to = target.join(name);
        io(fs::create_dir_all(to.parent().ok_or("文件没有父目录")?))?;
        io(fs::copy(path, to))?;
    }
    if hash_directory(source)?.0 != hash_directory(target)?.0 {
        return Err("复制校验失败".into());
    }
    Ok(())
}
fn remove_owned(path: &Path) -> Result<()> {
    if is_link(path) {
        #[cfg(windows)]
        {
            if junction::exists(path).map_err(|e| e.to_string())? {
                // junction::delete 只清除 reparse point，仍保留空目录壳。
                junction::delete(path).map_err(|e| e.to_string())?;
                return io(fs::remove_dir(path));
            }
            return io(fs::remove_dir(path));
        }
        #[cfg(not(windows))]
        {
            return io(fs::remove_file(path));
        }
    }
    if path.is_file() { io(fs::remove_file(path)) } else { io(fs::remove_dir_all(path)) }
}
fn create_link(source: &Path, target: &Path, mode: &str, allow_copy: bool) -> Result<String> {
    if exists(target) {
        return Err(format!("链接目标已存在：{}", display(target)));
    }
    if mode == "copy" {
        copy_directory(source, target)?;
        return Ok("copy".into());
    }
    let mut errors = vec![];
    if mode == "auto" {
        #[cfg(windows)]
        match junction::create(source, target) {
            Ok(()) => return Ok("junction".into()),
            Err(e) => {
                errors.push(format!("junction: {e}"));
                if exists(target) {
                    remove_owned(target)?;
                }
            }
        }
    }
    if mode == "symlink" || mode == "auto" {
        #[cfg(windows)]
        let result = std::os::windows::fs::symlink_dir(source, target);
        #[cfg(not(windows))]
        let result = std::os::unix::fs::symlink(source, target);
        match result {
            Ok(()) => return Ok("symlink".into()),
            Err(e) => errors.push(format!("symlink: {e}")),
        }
    }
    #[cfg(windows)]
    if mode != "auto" {
        match junction::create(source, target) {
            Ok(()) => return Ok("junction".into()),
            Err(e) => {
                errors.push(e.to_string());
                // 仅清理本次新建的暂存路径，避免失败留下目录壳阻止显式复制降级。
                if exists(target) {
                    remove_owned(target)?;
                }
            }
        }
    }
    if allow_copy {
        copy_directory(source, target)?;
        return Ok("copy".into());
    }
    Err(format!(
        "链接失败；可明确选择复制模式：{}",
        errors.join("；")
    ))
}
fn difference(a: &Path, b: &Path) -> Result<Vec<FileDifference>> {
    let left = files(a)?;
    let right = if b.is_dir() {
        files(b)?
    } else {
        BTreeMap::new()
    };
    let mut names: Vec<_> = left.keys().chain(right.keys()).cloned().collect();
    names.sort();
    names.dedup();
    let mut result = vec![];
    for name in names {
        let l = left.get(&name).map(|p| hash_file(p)).transpose()?;
        let r = right.get(&name).map(|p| hash_file(p)).transpose()?;
        if l == r {
            continue;
        }
        let preview = |path: Option<&PathBuf>| -> Result<Option<String>> {
            if let Some(path) = path {
                if io(fs::metadata(path))?.len() <= 65536 {
                    return Ok(String::from_utf8(io(fs::read(path))?).ok());
                }
            }
            Ok(None)
        };
        result.push(FileDifference {
            path: name.clone(),
            status: if l.is_none() {
                "target_only"
            } else if r.is_none() {
                "central_only"
            } else {
                "modified"
            }
            .into(),
            central: preview(left.get(&name))?,
            target: preview(right.get(&name))?,
        });
    }
    Ok(result)
}

impl SkillsService {
    pub fn new(data: PathBuf, home: PathBuf) -> Result<Self> {
        io(fs::create_dir_all(&data))?;
        let conn = Connection::open(data.join("skills.sqlite")).map_err(|e| e.to_string())?;
        conn.execute_batch("PRAGMA foreign_keys=ON;
          CREATE TABLE IF NOT EXISTS skills (id TEXT PRIMARY KEY,name TEXT NOT NULL,description TEXT NOT NULL,central_path TEXT NOT NULL,source_type TEXT NOT NULL,source_url TEXT,source_subpath TEXT,revision TEXT,content_hash TEXT NOT NULL,updated_at TEXT NOT NULL,payload TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS skill_targets (id TEXT PRIMARY KEY,skill_id TEXT NOT NULL,agent_id TEXT NOT NULL,scope TEXT NOT NULL,project_path TEXT,target_path TEXT NOT NULL UNIQUE,mode TEXT NOT NULL,status TEXT NOT NULL,last_synced_at TEXT,source_hash TEXT NOT NULL,error TEXT,payload TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS skill_repositories (id TEXT PRIMARY KEY,url TEXT NOT NULL,reference TEXT NOT NULL,checkout_path TEXT NOT NULL,last_checked_at TEXT NOT NULL,error TEXT,payload TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS skill_operations (at TEXT NOT NULL,operation TEXT NOT NULL,path TEXT NOT NULL,backup_path TEXT);") .map_err(|e| e.to_string())?;
        let central = home.join(".agents/skills");
        Ok(Self {
            data,
            home,
            central,
            conn,
            sync_plans: HashMap::new(),
            migration_plans: HashMap::new(),
            batch_plans: HashMap::new(),
        })
    }
    pub fn central_path(&self) -> String {
        display(&self.central)
    }
    pub fn expand(&self, value: &str) -> Result<PathBuf> {
        let p = if value == "~" {
            self.home.clone()
        } else if value.starts_with("~/") || value.starts_with("~\\") {
            self.home.join(&value[2..])
        } else {
            PathBuf::from(value)
        };
        absolute(&p)
    }
    fn stored<T: serde::de::DeserializeOwned>(&self, table: &str) -> Result<Vec<T>> {
        let sql = match table {
            "skills" => "SELECT payload FROM skills ORDER BY name",
            "skill_targets" => "SELECT payload FROM skill_targets",
            "skill_repositories" => "SELECT payload FROM skill_repositories",
            _ => return Err("未知元数据表".into()),
        };
        let mut stmt = self.conn.prepare(sql).map_err(|e| e.to_string())?;
        let values = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        values
            .map(|v| {
                serde_json::from_str(&v.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
            })
            .collect()
    }
    fn save_skill(&self, s: &Skill) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO skills VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                params![
                    s.id,
                    s.name,
                    s.description,
                    s.central_path,
                    s.source_type,
                    s.source_url,
                    s.source_subpath,
                    s.revision,
                    s.content_hash,
                    s.updated_at,
                    serde_json::to_string(s).map_err(|e| e.to_string())?
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn save_target(&self, s: &SkillTarget) -> Result<()> {
        self.conn.execute("INSERT OR REPLACE INTO skill_targets VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",params![s.id,s.skill_id,s.agent_id,s.scope,s.project_path,s.target_path,s.mode,s.status,s.last_synced_at,s.source_hash,s.error,serde_json::to_string(s).map_err(|e| e.to_string())?]).map_err(|e| e.to_string())?;
        Ok(())
    }
    fn save_repository(&self, r: &Repository) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO skill_repositories VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    r.id,
                    r.url,
                    r.reference,
                    r.checkout_path,
                    r.last_checked_at,
                    r.error,
                    serde_json::to_string(r).map_err(|e| e.to_string())?
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn log(&self, operation: &str, path: &Path, backup: Option<&Path>) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO skill_operations VALUES (?1,?2,?3,?4)",
                params![now(), operation, display(path), backup.map(display)],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn operations(&self) -> Result<Vec<serde_json::Value>> {
        let mut q = self.conn.prepare("SELECT at,operation,path,backup_path FROM skill_operations ORDER BY at DESC LIMIT 30").map_err(|e| e.to_string())?;
        let rows = q.query_map([],|r| Ok(serde_json::json!({"at":r.get::<_,String>(0)?,"operation":r.get::<_,String>(1)?,"path":r.get::<_,String>(2)?,"backupPath":r.get::<_,Option<String>>(3)?}))).map_err(|e| e.to_string())?;
        rows.map(|r| r.map_err(|e| e.to_string())).collect()
    }
    pub fn scan(&mut self) -> Result<ScanResult> {
        let old: Vec<Skill> = self.stored("skills")?;
        let mut skills = vec![];
        let mut diagnostics = vec![];
        if self.central.is_dir() {
            for entry in io(fs::read_dir(&self.central))? {
                let path = io(entry)?.path();
                if !path.is_dir()
                    || path
                        .file_name()
                        .is_some_and(|s| s.to_string_lossy().starts_with('.'))
                {
                    continue;
                }
                match read_skill(&path) {
                    Ok(mut s) => {
                        if path.file_name().and_then(|n| n.to_str()) != Some(&s.name) {
                            diagnostics.push(format!("{}：目录名与 name 不一致", display(&path)));
                            continue;
                        }
                        if skills
                            .iter()
                            .any(|x: &Skill| x.id.eq_ignore_ascii_case(&s.id))
                        {
                            diagnostics.push(format!("同名 Skill 冲突：{}", s.name));
                            continue;
                        }
                        if let Some(previous) = old.iter().find(|x| x.id == s.id) {
                            s.source_type = previous.source_type.clone();
                            s.source_url = previous.source_url.clone();
                            s.source_subpath = previous.source_subpath.clone();
                            s.revision = previous.revision.clone();
                            if previous.content_hash == s.content_hash {
                                s.updated_at = previous.updated_at.clone();
                            }
                        }
                        self.save_skill(&s)?;
                        skills.push(s);
                    }
                    Err(e) => diagnostics.push(e),
                }
            }
        }
        Ok(ScanResult {
            skills,
            diagnostics,
            central_path: self.central_path(),
        })
    }
    pub fn get(&self, id: &str) -> Result<Skill> {
        validate_name(id)?;
        let mut current = read_skill(&self.central.join(id))?;
        if current.name != id {
            return Err("Skill 名称与目录不一致".into());
        }
        if let Some(old) = self
            .stored::<Skill>("skills")?
            .into_iter()
            .find(|s| s.id == id)
        {
            current.source_type = old.source_type;
            current.source_url = old.source_url;
            current.source_subpath = old.source_subpath;
            current.revision = old.revision;
            current.updated_at = old.updated_at;
        }
        Ok(current)
    }
    fn backup_root(&self, label: &str) -> Result<PathBuf> {
        let root = self.data.join("skill-backups").join(format!(
            "{}-{}-{}",
            Utc::now().format("%Y%m%dT%H%M%S"),
            label,
            Uuid::new_v4()
        ));
        io(fs::create_dir_all(&root))?;
        Ok(root)
    }
    fn stage(&self, source: &Path, parent: &Path) -> Result<PathBuf> {
        io(fs::create_dir_all(parent))?;
        let stage = parent.join(format!(".toolbox-stage-{}", Uuid::new_v4()));
        if let Err(e) = copy_directory(source, &stage) {
            if exists(&stage) {
                if let Err(clean) = remove_owned(&stage) {
                    return Err(format!("{e}；暂存清理失败：{clean}"));
                }
            }
            return Err(e);
        }
        Ok(stage)
    }
    // 替换时先在同一文件系统暂存旧目录；备份记录用于人工恢复，失败时恢复原目标。
    fn publish(&self, stage: &Path, target: &Path) -> Result<Option<PathBuf>> {
        let backup = if exists(target) {
            Some(
                target
                    .parent()
                    .ok_or("目标无父目录")?
                    .join(format!(".toolbox-backup-{}", Uuid::new_v4())),
            )
        } else {
            None
        };
        if let Some(ref b) = backup {
            io(fs::rename(target, b))?;
        }
        if let Err(e) = fs::rename(stage, target) {
            if let Some(ref b) = backup {
                io(fs::rename(b, target))
                    .map_err(|r| format!("发布失败：{e}；恢复失败：{r}；备份：{}", display(b)))?;
            }
            return Err(e.to_string());
        }
        Ok(backup)
    }
    pub fn create(&mut self, name: &str, document: &str) -> Result<Skill> {
        validate_name(name)?;
        if metadata(document)?.0 != name {
            return Err("名称必须与 frontmatter name 相同".into());
        }
        io(fs::create_dir_all(&self.central))?;
        let target = self.central.join(name);
        if exists(&target) {
            return Err("同名 Skill 已存在，请使用编辑或冲突预览".into());
        }
        let stage = self
            .central
            .join(format!(".toolbox-stage-{}", Uuid::new_v4()));
        io(fs::create_dir(&stage))?;
        io(fs::write(stage.join("SKILL.md"), document))?;
        let mut s = read_skill(&stage)?;
        io(fs::rename(&stage, &target))?;
        s.central_path = display(&target);
        self.save_skill(&s)?;
        self.log("create", &target, None)?;
        Ok(s)
    }
    pub fn update(&mut self, id: &str, document: &str, expected_hash: &str) -> Result<Skill> {
        let current = self.get(id)?;
        if current.content_hash != expected_hash {
            return Err("中央库已变化，请重新读取后编辑".into());
        }
        if metadata(document)?.0 != id {
            return Err("编辑不能改变 name；请新建另一个 Skill".into());
        }
        let target = PathBuf::from(&current.central_path);
        if is_link(&target) {
            return Err("中央 Skill 是外部链接，请先导入为本地目录".into());
        }
        let stage = self.stage(&target, &self.central)?;
        let doc = document_path(&stage)?;
        io(fs::write(doc, document))?;
        let mut s = read_skill(&stage)?;
        let backup = self.publish(&stage, &target)?;
        s.central_path = display(&target);
        s.source_type = current.source_type;
        s.source_url = current.source_url;
        s.source_subpath = current.source_subpath;
        s.revision = current.revision;
        self.save_skill(&s)?;
        self.log("update", &target, backup.as_deref())?;
        Ok(s)
    }
    pub fn import_local(&mut self, path: &Path) -> Result<Skill> {
        let source = io(fs::canonicalize(path))?;
        let mut s = read_skill(&source)?;
        let target = self.central.join(&s.name);
        if same(&source, &target)? {
            self.save_skill(&s)?;
            return Ok(s);
        }
        if nested(&source, &self.central)? {
            return Err("导入来源与中央库不能互相嵌套".into());
        }
        if exists(&target) {
            if hash_directory(&target)?.0 == s.content_hash {
                return self.get(&s.id);
            }
            return Err("中央库存在同名不同内容，未覆盖；请在项目同步或迁移预览中逐项处理".into());
        }
        let stage = self.stage(&source, &self.central)?;
        self.publish(&stage, &target)?;
        s.central_path = display(&target);
        self.save_skill(&s)?;
        self.log("import", &target, None)?;
        Ok(s)
    }
    fn target_path(&self, id: &str, agent: &str, project: Option<&str>) -> Result<PathBuf> {
        validate_name(id)?;
        let folder = match agent {
            "codex" => ".agents",
            "codex-legacy" if project.is_none() => ".codex",
            "claude" => ".claude",
            "dsh" => ".dsh",
            _ => return Err("未知 Agent".into()),
        };
        let root = if let Some(p) = project {
            let path = absolute(Path::new(p))?;
            if !path.is_dir() {
                return Err("项目目录不存在".into());
            }
            if same(&path, &self.home)? {
                return Err("项目目录不能是用户主目录".into());
            }
            path
        } else {
            self.home.clone()
        };
        Ok(root.join(folder).join("skills").join(id))
    }
    fn managed(&self, path: &Path) -> Result<Option<SkillTarget>> {
        Ok(self
            .stored::<SkillTarget>("skill_targets")?
            .into_iter()
            .find(|t| key(Path::new(&t.target_path)) == key(path)))
    }
    fn owned_unchanged(&self, t: &SkillTarget) -> Result<bool> {
        let path = Path::new(&t.target_path);
        if !exists(path) {
            return Ok(true);
        }
        if t.mode == "copy" {
            Ok(!is_link(path) && hash_directory(path)?.0 == t.source_hash)
        } else {
            Ok(is_link(path) && same(path, &self.central.join(&t.skill_id))?)
        }
    }
    fn state_for(&self, skill: &Skill, path: &Path, base: Option<&SkillTarget>) -> Result<String> {
        if !exists(path) {
            return Ok("central_only".into());
        }
        if !path.is_dir() {
            return Ok("conflict".into());
        }
        let hash = hash_directory(path)?.0;
        if hash == skill.content_hash {
            return Ok("in_sync".into());
        }
        Ok(match base {
            Some(t) if hash == t.source_hash => "central_newer",
            Some(t) if skill.content_hash == t.source_hash => "target_newer",
            _ => "conflict",
        }
        .into())
    }
    pub fn targets(&self, id: &str, projects: &[String]) -> Result<Vec<SkillTarget>> {
        let s = self.get(id)?;
        let mut stored: Vec<SkillTarget> = self
            .stored::<SkillTarget>("skill_targets")?
            .into_iter()
            .filter(|t| t.skill_id == id)
            .collect();
        for project in std::iter::once(None).chain(projects.iter().map(|s| Some(s.as_str()))) {
            let agents = if project.is_none() {
                vec!["codex", "codex-legacy", "claude", "dsh"]
            } else {
                vec!["codex", "claude", "dsh"]
            };
            for agent in agents {
                let path = self.target_path(id, agent, project)?;
                if agent == "codex-legacy" && !exists(&path) {
                    continue;
                }
                if !stored
                    .iter()
                    .any(|t| key(Path::new(&t.target_path)) == key(&path))
                {
                    stored.push(SkillTarget {
                        id: Uuid::new_v4().to_string(),
                        skill_id: id.into(),
                        agent_id: agent.into(),
                        scope: if project.is_some() {
                            "project"
                        } else {
                            "global"
                        }
                        .into(),
                        project_path: project.map(str::to_owned),
                        target_path: display(&path),
                        mode: if agent == "codex" && project.is_none() {
                            "central"
                        } else if is_link(&path) {
                            #[cfg(windows)]
                            {
                                if junction::exists(&path).map_err(|e| e.to_string())? {
                                    "junction"
                                } else {
                                    "symlink"
                                }
                            }
                            #[cfg(not(windows))]
                            {
                                "symlink"
                            }
                        } else {
                            "copy"
                        }
                        .into(),
                        status: "central_only".into(),
                        last_synced_at: None,
                        source_hash: String::new(),
                        error: None,
                        managed: false,
                    });
                }
            }
        }
        for t in &mut stored {
            match self.state_for(&s, Path::new(&t.target_path), Some(t)) {
                Ok(status) => {
                    t.status = status;
                    t.error = None;
                }
                Err(e) => {
                    t.status = "error".into();
                    t.error = Some(e);
                }
            }
        }
        Ok(stored)
    }
    pub fn preview_sync(&mut self, request: SyncRequest) -> Result<SyncPreview> {
        if !["auto", "symlink", "junction", "copy"].contains(&request.mode.as_str()) {
            return Err("未知同步模式".into());
        }
        let s = self.get(&request.skill_id)?;
        let source = PathBuf::from(&s.central_path);
        let target = self.target_path(&s.id, &request.agent_id, request.project_path.as_deref())?;
        let direct = request.agent_id == "codex" && request.project_path.is_none();
        // 对尚不存在的目标及其父目录也解析现有祖先，拦截父目录链接引起的路径逃逸。
        if !direct && nested(&self.central, target.parent().ok_or("目标无父目录")?)? {
            return Err("中央库与目标目录不能互相嵌套".into());
        }
        let managed = self.managed(&target)?;
        let status = self.state_for(&s, &target, managed.as_ref())?;
        let own = managed
            .as_ref()
            .map(|t| self.owned_unchanged(t))
            .transpose()?
            .unwrap_or(false);
        let conflict = !direct && exists(&target) && !own;
        let preview = SyncPreview {
            id: Uuid::new_v4().to_string(),
            skill_id: s.id.clone(),
            source_path: s.central_path.clone(),
            target_path: display(&target),
            mode: if direct { "central" } else { &request.mode }.into(),
            status,
            conflict,
            managed: own,
            differences: if direct {
                vec![]
            } else {
                difference(&source, &target)?
            },
            target_document: document_path(&target)
                .ok()
                .map(fs::read_to_string)
                .transpose()
                .map_err(|e| e.to_string())?,
        };
        if self.sync_plans.len() > 100 {
            self.sync_plans.clear();
        }
        self.sync_plans.insert(
            preview.id.clone(),
            SavedSync {
                request,
                preview: preview.clone(),
                source_hash: s.content_hash,
                target_fingerprint: fingerprint(&target)?,
            },
        );
        Ok(preview)
    }
    pub fn apply_sync(&mut self, id: &str, choice: &str) -> Result<SkillTarget> {
        if !["central", "target", "skip"].contains(&choice) {
            return Err("未知冲突选择".into());
        }
        let plan = self.sync_plans.remove(id).ok_or("预览失效，请重新预览")?;
        if choice == "skip" {
            return Err("已跳过，无目录修改".into());
        }
        let mut s = self.get(&plan.request.skill_id)?;
        let target = PathBuf::from(&plan.preview.target_path);
        if s.content_hash != plan.source_hash || fingerprint(&target)? != plan.target_fingerprint {
            return Err("源或目标已变化，请重新预览".into());
        }
        let source = PathBuf::from(&s.central_path);
        if plan.preview.mode == "central" {
            return Ok(SkillTarget {
                id: s.id.clone(),
                skill_id: s.id,
                agent_id: "codex".into(),
                scope: "global".into(),
                project_path: None,
                target_path: display(&source),
                mode: "central".into(),
                status: "in_sync".into(),
                last_synced_at: None,
                source_hash: s.content_hash,
                error: None,
                managed: false,
            });
        }
        if nested(&self.central, target.parent().ok_or("目标无父目录")?)? {
            return Err("同步路径边界已变化".into());
        }
        let mut published: Vec<(PathBuf, Option<PathBuf>)> = vec![];
        self.conn
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| e.to_string())?;
        let outcome = (|| -> Result<SkillTarget> {
            if choice == "target" {
                let incoming = read_skill(&target)?;
                if incoming.id != s.id {
                    return Err("目标 frontmatter name 不一致".into());
                }
                if is_link(&source) {
                    return Err("中央内容为外部链接，不能替换".into());
                }
                let stage = self.stage(&target, &self.central)?;
                let backup = self.publish(&stage, &source)?;
                published.push((source.clone(), backup.clone()));
                self.log("keep-target", &source, backup.as_deref())?;
                s = self.get(&s.id)?;
                self.save_skill(&s)?;
            }
            io(fs::create_dir_all(target.parent().ok_or("目标无父目录")?))?;
            let stage = target
                .parent()
                .unwrap()
                .join(format!(".toolbox-stage-{}", Uuid::new_v4()));
            let mode =
                match create_link(&source, &stage, &plan.request.mode, plan.request.allow_copy) {
                    Ok(mode) => mode,
                    Err(e) => {
                        if exists(&stage) {
                            remove_owned(&stage)?;
                        }
                        return Err(e);
                    }
                };
            let backup = self.publish(&stage, &target)?;
            published.push((target.clone(), backup.clone()));
            let record = SkillTarget {
                id: self
                    .managed(&target)?
                    .map(|t| t.id)
                    .unwrap_or_else(|| Uuid::new_v4().to_string()),
                skill_id: s.id,
                agent_id: plan.request.agent_id,
                scope: if plan.request.project_path.is_some() {
                    "project"
                } else {
                    "global"
                }
                .into(),
                project_path: plan.request.project_path,
                target_path: display(&target),
                mode,
                status: "in_sync".into(),
                last_synced_at: Some(now()),
                source_hash: s.content_hash,
                error: None,
                managed: true,
            };
            self.save_target(&record)?;
            self.log("sync", &target, backup.as_deref())?;
            self.conn
                .execute_batch("COMMIT")
                .map_err(|e| e.to_string())?;
            Ok(record)
        })();
        match outcome {
            Ok(record) => Ok(record),
            Err(e) => {
                let mut errors = vec![];
                if let Err(r) = self.conn.execute_batch("ROLLBACK") {
                    errors.push(r.to_string());
                }
                for (path, backup) in published.iter().rev() {
                    if exists(path) {
                        if let Err(r) = remove_owned(path) {
                            errors.push(r);
                            continue;
                        }
                    }
                    if let Some(backup) = backup {
                        if let Err(r) = fs::rename(backup, path) {
                            errors.push(r.to_string());
                        }
                    }
                }
                Err(format!(
                    "同步失败：{e}；回滚错误：{}",
                    if errors.is_empty() {
                        "无".into()
                    } else {
                        errors.join("；")
                    }
                ))
            }
        }
    }
    pub fn remove_target(&mut self, id: &str) -> Result<OperationResult> {
        let t = self
            .stored::<SkillTarget>("skill_targets")?
            .into_iter()
            .find(|t| t.id == id)
            .ok_or("此目标不由工具管理")?;
        let path = PathBuf::from(&t.target_path);
        let expected = self.target_path(&t.skill_id, &t.agent_id, t.project_path.as_deref())?;
        if key(&expected) != key(&path)
            || key(&path) == key(&self.central.join(&t.skill_id))
            || nested(&self.central, path.parent().ok_or("目标无父目录")?)?
            || !self.owned_unchanged(&t)?
        {
            return Err("目标已被外部修改或路径不安全，未解除分发".into());
        }
        let backup = if exists(&path) {
            let b = path
                .parent()
                .unwrap()
                .join(format!(".toolbox-backup-{}", Uuid::new_v4()));
            io(fs::rename(&path, &b))?;
            Some(b)
        } else {
            None
        };
        if let Err(e) = self
            .conn
            .execute("DELETE FROM skill_targets WHERE id=?1", [id])
        {
            if let Some(ref b) = backup {
                io(fs::rename(b, &path))?;
            }
            return Err(e.to_string());
        }
        self.log("unlink", &path, backup.as_deref())?;
        Ok(OperationResult {
            message: "已解除分发，原目标保留备份".into(),
            backup_path: backup.as_deref().map(display),
        })
    }
    pub fn delete(
        &mut self,
        id: &str,
        permanent: bool,
        expected_hash: &str,
    ) -> Result<OperationResult> {
        let s = self.get(id)?;
        if s.content_hash != expected_hash {
            return Err("Skill 已变化，请重新确认".into());
        }
        let targets: Vec<SkillTarget> = self
            .stored::<SkillTarget>("skill_targets")?
            .into_iter()
            .filter(|t| t.skill_id == id)
            .collect();
        for t in &targets {
            if !self.owned_unchanged(t)? {
                return Err(format!("目标已修改，先处理冲突：{}", t.target_path));
            }
        }
        if permanent && is_link(Path::new(&s.central_path)) {
            return Err("中央库为外部链接，禁止删除其内容".into());
        }
        if permanent {
            for agent in ["codex-legacy", "claude", "dsh"] {
                let path = self.target_path(id, agent, None)?;
                if exists(&path)
                    && is_link(&path)
                    && same(&path, Path::new(&s.central_path))?
                    && self.managed(&path)?.is_none()
                {
                    return Err(format!(
                        "存在外部联接，不能删除中央内容；先在分发预览中接管或手动处理：{}",
                        display(&path)
                    ));
                }
            }
        }
        for t in targets {
            self.remove_target(&t.id)?;
        }
        let backup = if permanent {
            let root = self.backup_root("delete")?;
            let p = root.join(id);
            io(fs::rename(&s.central_path, &p))?;
            self.conn
                .execute("DELETE FROM skills WHERE id=?1", [id])
                .map_err(|e| e.to_string())?;
            self.log("delete", Path::new(&s.central_path), Some(&p))?;
            Some(display(&p))
        } else {
            None
        };
        Ok(OperationResult {
            message: if permanent {
                "中央内容已移出库，备份可恢复"
            } else {
                "已解除所有受管理的分发，中央内容保留"
            }
            .into(),
            backup_path: backup,
        })
    }
    pub fn export(&self, id: &str, destination: &Path) -> Result<OperationResult> {
        let s = self.get(id)?;
        let destination = absolute(destination)?.join(id);
        if nested(Path::new(&s.central_path), &destination)? {
            return Err("导出目标不能与中央内容嵌套".into());
        }
        copy_directory(Path::new(&s.central_path), &destination)?;
        Ok(OperationResult {
            message: format!("已导出到 {}", display(&destination)),
            backup_path: None,
        })
    }

    pub fn projects(&mut self, project: &str) -> Result<Vec<SkillTarget>> {
        let root = absolute(Path::new(project))?;
        if !root.is_dir() {
            return Err("项目目录不存在".into());
        }
        let skills = self.scan()?.skills;
        let mut result = vec![];
        for skill in &skills {
            result.extend(
                self.targets(&skill.id, &[project.into()])?
                    .into_iter()
                    .filter(|t| t.scope == "project"),
            );
        }
        for agent in ["codex", "claude", "dsh"] {
            let folder = self
                .target_path("placeholder", agent, Some(project))?
                .parent()
                .unwrap()
                .to_path_buf();
            if !folder.is_dir() {
                continue;
            }
            for entry in io(fs::read_dir(&folder))? {
                let path = io(entry)?.path();
                if !path.is_dir()
                    || path
                        .file_name()
                        .is_some_and(|s| s.to_string_lossy().starts_with('.'))
                {
                    continue;
                }
                let s = read_skill(&path)?;
                if !skills.iter().any(|x| x.id == s.id) {
                    result.push(SkillTarget {
                        id: Uuid::new_v4().to_string(),
                        skill_id: s.id,
                        agent_id: agent.into(),
                        scope: "project".into(),
                        project_path: Some(project.into()),
                        target_path: display(&path),
                        mode: if is_link(&path) { "symlink" } else { "copy" }.into(),
                        status: "target_only".into(),
                        last_synced_at: None,
                        source_hash: s.content_hash,
                        error: None,
                        managed: false,
                    });
                }
            }
        }
        Ok(result)
    }
    pub fn preview_migration(&mut self, settings: &SkillsSettings) -> Result<MigrationPreview> {
        let source = self.expand(&settings.migration.source_path)?;
        if nested(&source, &self.central)? {
            return Err("迁移来源与中央库不能嵌套".into());
        }
        let mut rows = vec![];
        let mut diagnostics = vec![];
        let mut fingerprints = vec![];
        let mut roots = vec![];
        for agent in ["codex-legacy", "claude", "dsh"] {
            let root = self
                .target_path("placeholder", agent, None)?
                .parent()
                .unwrap()
                .to_path_buf();
            if is_link(&root) && same(&root, &source)? {
                roots.push((root.clone(), fingerprint(&root)?));
            }
        }
        if source.is_dir() {
            for entry in io(fs::read_dir(&source))? {
                let path = io(entry)?.path();
                if !path.is_dir() {
                    continue;
                }
                if is_link(&path) {
                    diagnostics.push(format!("跳过迁移源中的联接：{}", display(&path)));
                    continue;
                }
                let s = match read_skill(&path) {
                    Ok(s) => s,
                    Err(e) => {
                        diagnostics.push(e);
                        continue;
                    }
                };
                if rows
                    .iter()
                    .any(|r: &MigrationRow| r.name.eq_ignore_ascii_case(&s.name))
                {
                    diagnostics.push(format!("重复迁移名称：{}", s.name));
                    continue;
                }
                let target = self.central.join(&s.name);
                if is_link(&target) {
                    diagnostics.push(format!("中央库含外部链接，先处理：{}", display(&target)));
                    continue;
                }
                let status = if !exists(&target) {
                    "source_only"
                } else if hash_directory(&target)?.0 == s.content_hash {
                    "identical"
                } else {
                    "conflict"
                };
                let mut links = vec![];
                for agent in ["codex-legacy", "claude", "dsh"] {
                    let link = self.target_path(&s.id, agent, None)?;
                    let root_link = roots
                        .iter()
                        .any(|(root, _)| link.parent().is_some_and(|p| key(p) == key(root)));
                    if (is_link(&link) || root_link) && same(&link, &path)? {
                        links.push(display(&link));
                    }
                }
                fingerprints.push((
                    fingerprint(&path)?,
                    fingerprint(&target)?,
                    links
                        .iter()
                        .map(|p| fingerprint(Path::new(p)))
                        .collect::<Result<Vec<_>>>()?,
                ));
                rows.push(MigrationRow {
                    name: s.name,
                    source_path: display(&path),
                    target_path: display(&target),
                    status: status.into(),
                    links,
                    differences: if status == "conflict" {
                        difference(&target, &path)?
                    } else {
                        vec![]
                    },
                });
            }
        }
        let preview = MigrationPreview {
            id: Uuid::new_v4().to_string(),
            source_path: display(&source),
            central_path: self.central_path(),
            rows,
            diagnostics,
            directory_links: roots.iter().map(|(path, _)| display(path)).collect(),
        };
        self.migration_plans.clear();
        self.migration_plans.insert(
            preview.id.clone(),
            SavedMigration {
                preview: preview.clone(),
                fingerprints,
                roots,
            },
        );
        Ok(preview)
    }
    pub fn apply_migration(
        &mut self,
        id: &str,
        resolutions: &[Resolution],
    ) -> Result<OperationResult> {
        let plan = self
            .migration_plans
            .remove(id)
            .ok_or("迁移预览失效，请重新预览")?;
        if !plan.preview.diagnostics.is_empty() {
            return Err("迁移诊断尚未处理，未修改目录".into());
        }
        for (root, fp) in &plan.roots {
            if fingerprint(root)? != *fp {
                return Err("Agent 目录联接已变化，请重新预览".into());
            }
        }
        let mut selected = vec![];
        for (row, (source_hash, target_hash, link_hashes)) in
            plan.preview.rows.iter().zip(&plan.fingerprints)
        {
            if fingerprint(Path::new(&row.source_path))? != *source_hash
                || fingerprint(Path::new(&row.target_path))? != *target_hash
            {
                return Err("迁移源或中央库已变化，请重新预览".into());
            }
            for (link, hash) in row.links.iter().zip(link_hashes) {
                if fingerprint(Path::new(link))? != *hash {
                    return Err("Agent 联接已变化，请重新预览".into());
                }
            }
            let choice = resolutions
                .iter()
                .find(|r| r.name == row.name)
                .map(|r| r.choice.as_str());
            let choice = match choice {
                Some("central" | "target" | "skip") => choice.unwrap(),
                None if row.status != "conflict" => "target",
                _ => return Err(format!("请明确处理冲突：{}", row.name)),
            };
            if choice == "central" && row.status == "source_only" {
                return Err(format!("中央库没有 {}，请选择迁移来源或跳过", row.name));
            }
            if choice != "skip" {
                selected.push((row, choice));
            }
        }
        if !plan.roots.is_empty() && selected.len() != plan.preview.rows.len() {
            return Err("整个 Agent skills 目录指向旧库，请处理全部技能后再替换目录入口".into());
        }
        let backup = self.backup_root("migration")?;
        // 所有备份与暂存先完成，发布过程中任意错误按逆序回滚。
        let mut staged = vec![];
        for (row, choice) in &selected {
            copy_directory(
                Path::new(&row.source_path),
                &backup.join("source").join(&row.name),
            )?;
            let target = PathBuf::from(&row.target_path);
            if target.is_dir() {
                copy_directory(&target, &backup.join("central").join(&row.name))?;
            }
            if *choice == "target" && row.status != "identical" {
                staged.push((
                    self.stage(Path::new(&row.source_path), &self.central)?,
                    target,
                ));
            }
        }
        let mut published: Vec<(PathBuf, Option<PathBuf>)> = vec![];
        self.conn
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| e.to_string())?;
        let run = (|| -> Result<()> {
            for (stage, target) in &staged {
                let old = self.publish(stage, target)?;
                published.push((target.clone(), old));
            }
            // 整目录联接先移入备份，避免逐项操作穿透到旧内容库。
            for (root, _) in &plan.roots {
                if key(&resolved(root.parent().ok_or("Agent 目录无父路径")?)?)
                    != key(&absolute(root.parent().unwrap())?)
                {
                    return Err("Agent 上级目录为联接，禁止迁移".into());
                }
                let old = root
                    .parent()
                    .unwrap()
                    .join(format!(".toolbox-backup-{}", Uuid::new_v4()));
                io(fs::rename(root, &old))?;
                published.push((root.clone(), Some(old.clone())));
                io(fs::create_dir(root))?;
                self.log("migration-root", root, Some(&old))?;
            }
            for (row, _) in &selected {
                for link in &row.links {
                    let path = PathBuf::from(link);
                    if nested(&self.central, path.parent().ok_or("联接无父目录")?)? {
                        return Err("联接目录与中央库嵌套".into());
                    }
                    if path.starts_with(self.home.join(".codex")) {
                        if exists(&path) {
                            let old = path
                                .parent()
                                .unwrap()
                                .join(format!(".toolbox-backup-{}", Uuid::new_v4()));
                            io(fs::rename(&path, &old))?;
                            published.push((path.clone(), Some(old.clone())));
                            self.log("migration-codex-entry", &path, Some(&old))?;
                        }
                        self.conn
                            .execute("DELETE FROM skill_targets WHERE target_path=?1", [link])
                            .map_err(|e| e.to_string())?;
                        continue;
                    }
                    let stage = path
                        .parent()
                        .unwrap()
                        .join(format!(".toolbox-stage-{}", Uuid::new_v4()));
                    let mode = create_link(Path::new(&row.target_path), &stage, "auto", false)?;
                    let old = self.publish(&stage, &path)?;
                    published.push((path.clone(), old));
                    let agent = if path.starts_with(self.home.join(".codex")) {
                        "codex-legacy"
                    } else if path.starts_with(self.home.join(".claude")) {
                        "claude"
                    } else {
                        "dsh"
                    };
                    let s = read_skill(Path::new(&row.target_path))?;
                    self.save_target(&SkillTarget {
                        id: Uuid::new_v4().to_string(),
                        skill_id: row.name.clone(),
                        agent_id: agent.into(),
                        scope: "global".into(),
                        project_path: None,
                        target_path: link.clone(),
                        mode,
                        status: "in_sync".into(),
                        last_synced_at: Some(now()),
                        source_hash: s.content_hash,
                        error: None,
                        managed: true,
                    })?;
                }
            }
            self.scan()?;
            self.log(
                "migration",
                Path::new(&plan.preview.source_path),
                Some(&backup),
            )?;
            self.conn
                .execute_batch("COMMIT")
                .map_err(|e| e.to_string())?;
            Ok(())
        })();
        if let Err(e) = run {
            let mut restore_errors = vec![];
            if let Err(r) = self.conn.execute_batch("ROLLBACK") {
                restore_errors.push(r.to_string());
            }
            for (target, old) in published.iter().rev() {
                if exists(target) {
                    if let Err(r) = remove_owned(target) {
                        restore_errors.push(r);
                        continue;
                    }
                }
                if let Some(old) = old {
                    if let Err(r) = fs::rename(old, target) {
                        restore_errors.push(r.to_string());
                    }
                }
            }
            return Err(format!(
                "迁移失败：{e}；恢复错误：{}；备份：{}",
                restore_errors.join("；"),
                display(&backup)
            ));
        }
        Ok(OperationResult {
            message: format!("已迁移 {} 个 Skill；迁移源保留", selected.len()),
            backup_path: Some(display(&backup)),
        })
    }
    pub fn repositories(&self) -> Result<Vec<Repository>> {
        self.stored("skill_repositories")
    }
    pub fn checkout(&mut self, url: &str, reference: &str) -> Result<(Repository, Vec<Candidate>)> {
        validate_github(url)?;
        validate_ref(reference)?;
        let root = self
            .data
            .join("repositories")
            .join(Uuid::new_v4().to_string());
        io(fs::create_dir_all(&root))?;
        let checkout = root.join("checkout");
        let mut repo = Repository {
            id: Uuid::new_v4().to_string(),
            url: url.trim_end_matches('/').into(),
            reference: reference.into(),
            checkout_path: display(&checkout),
            revision: String::new(),
            last_checked_at: now(),
            error: None,
            update_available: false,
        };
        let outcome = (|| -> Result<Vec<Candidate>> {
            git(&root, &["clone", "--no-checkout", "--", url, "checkout"])?;
            git(&checkout, &["checkout", "--detach", reference])?;
            repo.revision = git(&checkout, &["rev-parse", "HEAD"])?;
            let mut candidates = vec![];
            for entry in WalkDir::new(&checkout)
                .follow_links(false)
                .into_iter()
                .filter_entry(|e| e.file_name() != ".git")
            {
                let entry = entry.map_err(|e| e.to_string())?;
                if entry.file_type().is_file()
                    && ["SKILL.md", "skill.md"]
                        .contains(&entry.file_name().to_string_lossy().as_ref())
                {
                    let path = entry.path().parent().ok_or("Skill 文件无父目录")?;
                    let s = read_skill(path)?;
                    let relative =
                        display(path.strip_prefix(&checkout).map_err(|e| e.to_string())?)
                            .replace('\\', "/");
                    if !candidates.iter().any(|c: &Candidate| c.path == relative) {
                        candidates.push(Candidate {
                            path: relative,
                            name: s.name,
                            description: s.description,
                            content_hash: s.content_hash,
                        });
                    }
                }
            }
            Ok(candidates)
        })();
        match outcome {
            Ok(c) => {
                self.save_repository(&repo)?;
                Ok((repo, c))
            }
            Err(e) => {
                repo.error = Some(e.clone());
                self.save_repository(&repo)?;
                Err(e)
            }
        }
    }
    pub fn import_repository(&mut self, id: &str, paths: &[String]) -> Result<Vec<Skill>> {
        let repo = self
            .repositories()?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or("仓库未登记")?;
        if repo.error.is_some() {
            return Err("仓库 checkout 失败，请重新导入".into());
        }
        let checkout = PathBuf::from(&repo.checkout_path);
        if git(&checkout, &["rev-parse", "HEAD"])? != repo.revision
            || !git(&checkout, &["status", "--porcelain"])?.is_empty()
        {
            return Err("checkout 已被修改，请重新获取仓库".into());
        }
        let mut selected = vec![];
        // 预检所有选择，避免同名冲突在批次中途出现。
        for relative in paths {
            let p = Path::new(relative);
            if p.is_absolute()
                || p.components().any(|c| {
                    matches!(
                        c,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
            {
                return Err("仓库子路径越界".into());
            }
            let source = checkout.join(p);
            let canonical = io(fs::canonicalize(&source))?;
            if !canonical.starts_with(io(fs::canonicalize(&checkout))?) {
                return Err("仓库子路径经链接越界".into());
            }
            let s = read_skill(&source)?;
            if selected
                .iter()
                .any(|(_, previous): &(String, Skill)| previous.id.eq_ignore_ascii_case(&s.id))
            {
                return Err("选择的 Skill 同名，请分别处理".into());
            }
            let target = self.central.join(&s.id);
            if exists(&target) && hash_directory(&target)?.0 != s.content_hash {
                return Err(format!("同名 Skill 内容冲突，未覆盖：{}", s.name));
            }
            selected.push((relative.clone(), s));
        }
        let mut result = vec![];
        for (path, _) in selected {
            let mut s = self.import_local(&checkout.join(&path))?;
            s.source_type = "github".into();
            s.source_url = Some(repo.url.clone());
            s.source_subpath = Some(path);
            s.revision = Some(repo.revision.clone());
            self.save_skill(&s)?;
            result.push(s);
        }
        Ok(result)
    }
    pub fn check_updates(&mut self) -> Result<Vec<Repository>> {
        let mut repos = self.repositories()?;
        for repo in &mut repos {
            repo.last_checked_at = now();
            let outcome = (|| -> Result<String> {
                validate_github(&repo.url)?;
                validate_ref(&repo.reference)?;
                let checkout = PathBuf::from(&repo.checkout_path);
                if !git(&checkout, &["status", "--porcelain"])?.is_empty() {
                    return Err("checkout 存在本地修改，未更新".into());
                }
                git(
                    &checkout,
                    &["fetch", "--no-tags", "--", &repo.url, &repo.reference],
                )?;
                git(&checkout, &["rev-parse", "FETCH_HEAD"])
            })();
            match outcome {
                Ok(commit) => {
                    repo.update_available = commit != repo.revision;
                    repo.error = None;
                }
                Err(e) => repo.error = Some(e),
            }
            self.save_repository(repo)?;
        }
        Ok(repos)
    }
}

fn validate_github(url: &str) -> Result<()> {
    let rest = url
        .strip_prefix("https://github.com/")
        .ok_or("只支持 HTTPS GitHub 仓库 URL")?;
    let parts: Vec<_> = rest.trim_end_matches('/').split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|p| {
            p.is_empty()
                || p.starts_with('.')
                || !p
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        })
    {
        return Err("仓库 URL 必须是 https://github.com/owner/repo".into());
    }
    Ok(())
}
fn validate_ref(reference: &str) -> Result<()> {
    if reference.is_empty()
        || reference.starts_with('-')
        || reference.contains("..")
        || !reference
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_/ .".contains(&c) && c != b' ')
    {
        return Err("无效 Git ref".into());
    }
    Ok(())
}
fn git(directory: &Path, args: &[&str]) -> Result<String> {
    let output = std::env::temp_dir().join(format!(".toolbox-git-output-{}", Uuid::new_v4()));
    let stderr = std::env::temp_dir().join(format!(".toolbox-git-error-{}", Uuid::new_v4()));
    let mut command = Command::new("git");
    command
        .args([
            "-c",
            "core.hooksPath=NUL",
            "-c",
            "credential.interactive=false",
            "-c",
            "core.autocrlf=false",
            "-c",
            "core.longpaths=true",
        ])
        .args(args)
        .current_dir(directory)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(io(fs::File::create(&output))?)
        .stderr(io(fs::File::create(&stderr))?);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|e| format!("Git 不可用：{e}"))?;
    let start = Instant::now();
    let status = loop {
        if let Some(status) = io(child.try_wait())? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(120) {
            io(child.kill())?;
            io(child.wait())?;
            return Err("Git 操作超时（120 秒）".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let text = io(fs::read_to_string(&output))?;
    let error = io(fs::read_to_string(&stderr))?;
    io(fs::remove_file(output))?;
    io(fs::remove_file(stderr))?;
    if !status.success() {
        return Err(format!(
            "Git 操作失败：{}",
            error.chars().take(4000).collect::<String>()
        ));
    }
    Ok(text.trim().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, SkillsService) {
        let t = tempfile::tempdir().unwrap();
        let home = t.path().join("home");
        fs::create_dir(&home).unwrap();
        let s = SkillsService::new(t.path().join("data"), home).unwrap();
        (t, s)
    }
    fn doc(name: &str, value: &str) -> String {
        format!("---\nname: {name}\ndescription: |\n  第一行\n  第二行\n---\n\n{value}\n")
    }
    fn write_skill(path: &Path, name: &str, value: &str) {
        fs::create_dir_all(path).unwrap();
        fs::write(path.join("skill.md"), doc(name, value)).unwrap();
        fs::create_dir(path.join("refs")).unwrap();
        fs::write(path.join("refs/support.txt"), "支持文件").unwrap();
    }
    fn request(id: &str, mode: &str) -> SyncRequest {
        SyncRequest {
            skill_id: id.into(),
            agent_id: "claude".into(),
            project_path: None,
            mode: mode.into(),
            allow_copy: false,
        }
    }
    #[test]
    fn frontmatter_and_names() {
        let (name, description) = metadata(&doc("valid-skill", "body")).unwrap();
        assert_eq!(name, "valid-skill");
        assert_eq!(description, "第一行\n第二行");
        for name in [
            "../x",
            "CON",
            "NUL",
            "synced",
            "anthropic-skills",
            "foo bar",
            ".hidden",
            "com1",
        ] {
            assert!(validate_name(name).is_err());
        }
        assert!(metadata("---\nname: x\n---").is_err());
        assert!(metadata("plain").is_err());
    }
    #[test]
    fn local_import_deduplicates_and_scan_is_idempotent() {
        let (t, mut s) = fixture();
        let path = t.path().join("incoming");
        write_skill(&path, "example", "v1");
        let first = s.import_local(&path).unwrap();
        let second = s.import_local(&path).unwrap();
        assert_eq!(first.content_hash, second.content_hash);
        assert_eq!(s.scan().unwrap().skills.len(), 1);
        assert_eq!(s.scan().unwrap().skills[0].updated_at, first.updated_at);
        fs::write(path.join("skill.md"), doc("example", "v2")).unwrap();
        assert!(s.import_local(&path).is_err());
        assert_eq!(s.get("example").unwrap().content_hash, first.content_hash);
        assert!(s.import_local(&s.central.join("example/refs")).is_err());
    }
    #[test]
    fn copy_states_conflicts_and_preview_staleness() {
        let (_t, mut s) = fixture();
        let first = s.create("example", &doc("example", "v1")).unwrap();
        let p = s.preview_sync(request("example", "copy")).unwrap();
        assert_eq!(p.status, "central_only");
        let target = s.apply_sync(&p.id, "central").unwrap();
        assert_eq!(target.mode, "copy");
        assert_eq!(
            s.targets("example", &[])
                .unwrap()
                .iter()
                .find(|t| t.agent_id == "claude")
                .unwrap()
                .status,
            "in_sync"
        );
        let next = s
            .update("example", &doc("example", "v2"), &first.content_hash)
            .unwrap();
        assert_eq!(
            s.targets("example", &[])
                .unwrap()
                .iter()
                .find(|t| t.agent_id == "claude")
                .unwrap()
                .status,
            "central_newer"
        );
        let p = s.preview_sync(request("example", "copy")).unwrap();
        s.apply_sync(&p.id, "central").unwrap();
        fs::write(
            Path::new(&target.target_path).join("SKILL.md"),
            doc("example", "local"),
        )
        .unwrap();
        assert_eq!(
            s.targets("example", &[])
                .unwrap()
                .iter()
                .find(|t| t.agent_id == "claude")
                .unwrap()
                .status,
            "target_newer"
        );
        assert!(s.remove_target(&target.id).is_err());
        let p = s.preview_sync(request("example", "copy")).unwrap();
        assert!(p.conflict);
        assert!(!p.differences.is_empty());
        fs::write(
            Path::new(&target.target_path).join("SKILL.md"),
            doc("example", "changed-again"),
        )
        .unwrap();
        assert!(s.apply_sync(&p.id, "central").is_err());
        let p = s.preview_sync(request("example", "copy")).unwrap();
        s.apply_sync(&p.id, "target").unwrap();
        assert!(s.get("example").unwrap().document.contains("changed-again"));
        assert_ne!(s.get("example").unwrap().content_hash, next.content_hash);
        assert!(s.scan().unwrap().diagnostics.is_empty());
    }
    #[test]
    fn external_directory_and_link_are_never_silently_removed() {
        let (t, mut s) = fixture();
        s.create("example", &doc("example", "central")).unwrap();
        let target = s.target_path("example", "claude", None).unwrap();
        write_skill(&target, "example", "external");
        let p = s.preview_sync(request("example", "copy")).unwrap();
        assert!(p.conflict);
        assert!(s.remove_target("unknown").is_err());
        assert!(fs::read_to_string(target.join("skill.md"))
            .unwrap()
            .contains("external"));
        let unrelated = t.path().join("unrelated");
        write_skill(&unrelated, "example", "foreign");
        let dsh = s.target_path("example", "dsh", None).unwrap();
        fs::create_dir_all(dsh.parent().unwrap()).unwrap();
        create_link(&unrelated, &dsh, "symlink", false).unwrap();
        let p = s
            .preview_sync(SyncRequest {
                agent_id: "dsh".into(),
                ..request("example", "copy")
            })
            .unwrap();
        assert!(p.conflict);
        assert!(unrelated.is_dir());
        let copied = s.apply_sync(&p.id, "central").unwrap();
        assert_eq!(copied.mode, "copy");
        assert!(fs::read_to_string(unrelated.join("skill.md"))
            .unwrap()
            .contains("foreign"));
        s.remove_target(&copied.id).unwrap();
        assert!(s.central.join("example").is_dir());
        assert!(!exists(&dsh));
    }
    #[test]
    fn windows_link_modes_and_unlink_preserve_source() {
        let (_t, mut s) = fixture();
        s.create("example", &doc("example", "v1")).unwrap();
        for mode in ["symlink", "junction", "copy"] {
            let p = s.preview_sync(request("example", mode)).unwrap();
            let target = s.apply_sync(&p.id, "central").unwrap();
            if mode != "copy" {
                assert!(is_link(Path::new(&target.target_path)));
            }
            s.remove_target(&target.id).unwrap();
            assert!(s.get("example").is_ok());
        }
        let p = s
            .preview_sync(SyncRequest {
                agent_id: "codex".into(),
                ..request("example", "symlink")
            })
            .unwrap();
        assert_eq!(p.mode, "central");
        assert_eq!(s.apply_sync(&p.id, "central").unwrap().mode, "central");
    }
    #[test]
    fn nested_paths_and_parent_links_are_rejected() {
        let (_t, mut s) = fixture();
        s.create("example", &doc("example", "v1")).unwrap();
        let project = s.central.join("example/project");
        fs::create_dir(&project).unwrap();
        assert!(s
            .preview_sync(SyncRequest {
                project_path: Some(display(&project)),
                ..request("example", "copy")
            })
            .is_err());
        let parent = s.home.join(".claude/skills");
        fs::create_dir_all(parent.parent().unwrap()).unwrap();
        create_link(&s.central, &parent, "symlink", false).unwrap();
        assert!(s.preview_sync(request("example", "copy")).is_err());
    }
    #[test]
    fn migration_requires_conflict_resolution_and_preserves_source_backup() {
        let (_t, mut s) = fixture();
        let source = s.home.join(".skills-manager/skills/example");
        write_skill(&source, "example", "old");
        s.create("example", &doc("example", "current")).unwrap();
        let link = s.target_path("example", "claude", None).unwrap();
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        create_link(&source, &link, "junction", false).unwrap();
        let p = s.preview_migration(&SkillsSettings::default()).unwrap();
        assert_eq!(p.rows[0].status, "conflict");
        assert_eq!(p.rows[0].links.len(), 1);
        assert!(s.apply_migration(&p.id, &[]).is_err());
        assert!(
            same(&link, &source).unwrap(),
            "link={} source={} target={:?}",
            display(&link),
            display(&source),
            fs::read_link(&link)
        );
        let p = s.preview_migration(&SkillsSettings::default()).unwrap();
        let r = s
            .apply_migration(
                &p.id,
                &[Resolution {
                    name: "example".into(),
                    choice: "target".into(),
                }],
            )
            .unwrap();
        assert!(same(&link, &s.central.join("example")).unwrap());
        assert!(source.is_dir());
        assert!(Path::new(&r.backup_path.unwrap())
            .join("source/example/skill.md")
            .is_file());
        let p = s.preview_migration(&SkillsSettings::default()).unwrap();
        assert_eq!(p.rows[0].status, "identical");
        assert!(p.rows[0].links.is_empty());
    }
    #[test]
    fn migration_changed_source_aborts_before_mutation() {
        let (_t, mut s) = fixture();
        let source = s.home.join(".skills-manager/skills/example");
        write_skill(&source, "example", "old");
        let p = s.preview_migration(&SkillsSettings::default()).unwrap();
        fs::write(source.join("skill.md"), doc("example", "changed")).unwrap();
        assert!(s.apply_migration(&p.id, &[]).is_err());
        assert!(!s.central.join("example").exists());
        assert!(source.is_dir());
    }
    #[test]
    fn project_target_only_and_export_preserve_support_files() {
        let (t, mut s) = fixture();
        let project = t.path().join("project");
        let target = project.join(".dsh/skills/example");
        write_skill(&target, "example", "project");
        let targets = s.projects(&display(&project)).unwrap();
        assert_eq!(targets[0].status, "target_only");
        let imported = s.import_local(&target).unwrap();
        let export = t.path().join("export");
        fs::create_dir(&export).unwrap();
        s.export(&imported.id, &export).unwrap();
        assert!(export.join("example/refs/support.txt").is_file());
        assert!(s.export(&imported.id, &export).is_err());
    }
    #[test]
    fn git_inputs_and_failures_are_explicit() {
        assert!(validate_github("https://github.com/org/repo").is_ok());
        for url in [
            "file:///tmp/x",
            "https://github.com/org/repo/tree/main",
            "https://user:pass@github.com/org/repo",
            "https://github.com/org/repo?x=y",
        ] {
            assert!(validate_github(url).is_err());
        }
        for reference in ["--upload-pack=x", "../main", "x;cmd", "x y"] {
            assert!(validate_ref(reference).is_err());
        }
        let t = tempfile::tempdir().unwrap();
        assert!(git(t.path(), &["rev-parse", "HEAD"]).is_err());
    }
    #[test]
    fn sync_database_failure_restores_both_versions() {
        let (_t, mut s) = fixture();
        s.create("example", &doc("example", "central")).unwrap();
        let target = s.target_path("example", "claude", None).unwrap();
        write_skill(&target, "example", "target");
        let before = s.get("example").unwrap().content_hash;
        let target_hash = hash_directory(&target).unwrap().0;
        s.conn.execute_batch("CREATE TRIGGER fail_target BEFORE INSERT ON skill_targets BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
        let p = s.preview_sync(request("example", "copy")).unwrap();
        assert!(s.apply_sync(&p.id, "target").is_err());
        assert_eq!(s.get("example").unwrap().content_hash, before);
        assert_eq!(hash_directory(&target).unwrap().0, target_hash);
        assert!(s.stored::<SkillTarget>("skill_targets").unwrap().is_empty());
    }
    #[test]
    fn migration_database_failure_restores_links_and_keeps_backup() {
        let (_t, mut s) = fixture();
        let source = s.home.join(".skills-manager/skills/example");
        write_skill(&source, "example", "old");
        s.create("example", &doc("example", "current")).unwrap();
        let before = s.get("example").unwrap().content_hash;
        let link = s.target_path("example", "claude", None).unwrap();
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        create_link(&source, &link, "junction", false).unwrap();
        s.conn.execute_batch("CREATE TRIGGER fail_target BEFORE INSERT ON skill_targets BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
        let p = s.preview_migration(&SkillsSettings::default()).unwrap();
        let e = s
            .apply_migration(
                &p.id,
                &[Resolution {
                    name: "example".into(),
                    choice: "target".into(),
                }],
            )
            .unwrap_err();
        assert!(e.contains("injected failure"));
        assert!(
            same(&link, &source).unwrap(),
            "{e}; target={:?}; source={:?}",
            fs::canonicalize(&link),
            fs::canonicalize(&source)
        );
        assert_eq!(s.get("example").unwrap().content_hash, before);
        assert!(s.data.join("skill-backups").is_dir());
        assert!(source.join("skill.md").is_file());
    }
}
