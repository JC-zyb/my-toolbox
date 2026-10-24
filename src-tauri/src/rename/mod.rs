mod executor;
mod rules;
pub use rules::Rules;
use rules::{natural_cmp, split_name, Sort, Transformer};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    time::{Duration, Instant, SystemTime},
};

const PREVIEW_TTL: Duration = Duration::from_secs(15 * 60);
const MAX_FILES: usize = 50_000;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub directory: String,
    pub rules: Rules,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub from: String,
    pub to: String,
    pub from_name: String,
    pub to_name: String,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub id: String,
    pub directory: String,
    pub rows: Vec<Row>,
    pub occupied_names: Vec<String>,
    pub skipped: usize,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoInfo {
    pub id: String,
    pub count: usize,
    pub directory: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub count: usize,
    pub undo: UndoInfo,
}
#[derive(Debug, Clone, PartialEq)]
struct Stamp {
    identity: String,
    len: u64,
    modified: SystemTime,
}
#[derive(Clone)]
struct Saved {
    plan: Plan,
    root_identity: String,
    stamps: HashMap<String, Stamp>,
    created: Instant,
}
#[derive(Default)]
pub struct RenameService {
    plan: Option<Saved>,
    undo: Option<Saved>,
}

impl RenameService {
    pub fn preview(&mut self, request: Request) -> Result<Plan, String> {
        self.plan = None;
        let transformer = Transformer::new(&request.rules)?;
        let root = Path::new(&request.directory)
            .canonicalize()
            .map_err(|e| format!("无法读取目录：{e}"))?;
        if !root.is_dir() {
            return Err("请选择目录".into());
        }
        let root_identity = executor::identity(&root)?;
        let mut entries = vec![];
        let mut occupied_names = vec![];
        let mut skipped = 0;
        let extensions: HashSet<_> = request
            .rules
            .extensions
            .split([',', ';', ' ', '，'])
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_start_matches('.').to_lowercase())
            .collect();
        for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "目录存在非 UTF-8 名称，无法安全预览")?;
            occupied_names.push(name.clone());
            if occupied_names.len() > MAX_FILES {
                return Err("目录超过 50000 项，请先缩小处理范围".into());
            }
            let m = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
            if !m.is_file()
                || executor::is_link(&m)
                || !request.rules.include_hidden && executor::is_hidden(&m, &name)
                || !name
                    .to_lowercase()
                    .contains(&request.rules.name_filter.to_lowercase())
                || !extensions.is_empty()
                    && !extensions.contains(&split_name(&name).1.to_lowercase())
            {
                skipped += 1;
                continue;
            }
            entries.push((entry.path(), name, m.modified().map_err(|e| e.to_string())?));
        }
        entries.sort_by(|a, b| {
            let order = match request.rules.sort {
                Sort::Natural => natural_cmp(&a.1, &b.1),
                Sort::Name => a.1.to_lowercase().cmp(&b.1.to_lowercase()),
                Sort::Modified => a.2.cmp(&b.2),
            }
            .then(a.1.cmp(&b.1));
            if request.rules.descending {
                order.reverse()
            } else {
                order
            }
        });
        let mut stamps = HashMap::new();
        let mut rows = vec![];
        for (i, (path, name, _)) in entries.into_iter().enumerate() {
            let from = display(&path);
            stamps.insert(from.clone(), executor::stamp(&path)?);
            let to_name = transformer.transform(&name, i)?;
            rows.push(Row {
                from,
                to: display(&root.join(&to_name)),
                from_name: name,
                error: rules::invalid_name(&to_name),
                to_name,
            });
        }
        let plan = Plan {
            id: uuid::Uuid::new_v4().to_string(),
            directory: display(&root),
            rows,
            occupied_names,
            skipped,
        };
        self.plan = Some(Saved {
            plan: plan.clone(),
            stamps,
            root_identity,
            created: Instant::now(),
        });
        Ok(plan)
    }
    pub fn apply(&mut self, id: &str, selected: &[String]) -> Result<ApplyResult, String> {
        let saved = self
            .plan
            .take()
            .filter(|s| s.plan.id == id)
            .ok_or("预览已失效，请重新预览")?;
        if saved.created.elapsed() > PREVIEW_TTL {
            return Err("预览已过期，请重新预览".into());
        }
        let selected_set: HashSet<_> = selected.iter().collect();
        if selected_set.is_empty()
            || selected_set.len() != selected.len()
            || selected.iter().any(|p| !saved.stamps.contains_key(p))
        {
            return Err("请选择预览中的文件，不允许重复选择".into());
        }
        let rows: Vec<_> = saved
            .plan
            .rows
            .iter()
            .filter(|r| selected_set.contains(&r.from) && r.from != r.to)
            .cloned()
            .collect();
        if rows.is_empty() {
            return Err("所选文件没有变化".into());
        }
        validate(&saved, &rows)?;
        executor::execute(&rows, &saved.stamps)?;
        let reverse: Vec<_> = rows
            .iter()
            .map(|r| Row {
                from: r.to.clone(),
                to: r.from.clone(),
                from_name: r.to_name.clone(),
                to_name: r.from_name.clone(),
                error: None,
            })
            .collect();
        let undo_id = uuid::Uuid::new_v4().to_string();
        let undo = UndoInfo {
            id: undo_id.clone(),
            count: rows.len(),
            directory: saved.plan.directory.clone(),
        };
        // 重命名不改变文件身份、长度与写入时间；沿用移动前快照，避免把外部写入当成可撤销版本。
        let stamps = rows
            .iter()
            .map(|r| (r.to.clone(), saved.stamps[&r.from].clone()))
            .collect();
        self.undo = Some(Saved {
            plan: Plan {
                id: undo_id,
                directory: saved.plan.directory,
                rows: reverse,
                occupied_names: vec![],
                skipped: 0,
            },
            stamps,
            root_identity: saved.root_identity,
            created: Instant::now(),
        });
        Ok(ApplyResult {
            count: rows.len(),
            undo,
        })
    }
    pub fn undo_info(&self) -> Option<UndoInfo> {
        self.undo.as_ref().map(|s| UndoInfo {
            id: s.plan.id.clone(),
            count: s.plan.rows.len(),
            directory: s.plan.directory.clone(),
        })
    }
    pub fn undo_preview(&self, id: &str) -> Result<Plan, String> {
        let saved = self
            .undo
            .as_ref()
            .filter(|s| s.plan.id == id)
            .ok_or("撤销记录已失效")?;
        validate(saved, &saved.plan.rows)?;
        Ok(saved.plan.clone())
    }
    pub fn undo(&mut self, id: &str) -> Result<usize, String> {
        let saved = self
            .undo
            .as_ref()
            .filter(|s| s.plan.id == id)
            .ok_or("撤销记录已失效")?;
        validate(saved, &saved.plan.rows)?;
        executor::execute(&saved.plan.rows, &saved.stamps)?;
        let count = saved.plan.rows.len();
        self.undo = None;
        self.plan = None;
        Ok(count)
    }
}
fn validate(saved: &Saved, rows: &[Row]) -> Result<(), String> {
    let root = Path::new(&saved.plan.directory);
    if root
        .canonicalize()
        .map_err(|e| format!("预览目录已改变：{e}"))?
        != root
        || executor::identity(root)? != saved.root_identity
    {
        return Err("预览目录已改变，请重新预览".into());
    }
    for row in rows {
        if Path::new(&row.from).parent() != Some(root) || Path::new(&row.to).parent() != Some(root)
        {
            return Err("目标必须位于预览目录内".into());
        }
        if let Some(e) = rules::invalid_name(&row.to_name) {
            return Err(format!("{}：{e}", row.to_name));
        }
        if executor::stamp(Path::new(&row.from))
            .map_err(|e| format!("文件已改变：{}：{e}", row.from_name))?
            != saved.stamps[&row.from]
        {
            return Err(format!("文件已改变，请重新预览：{}", row.from_name));
        }
    }
    let mut targets = HashSet::new();
    let mut moving: HashMap<String, HashSet<String>> = HashMap::new();
    for row in rows {
        moving
            .entry(row.from_name.to_lowercase())
            .or_default()
            .insert(row.from_name.clone());
    }
    let mut occupied: HashMap<String, Vec<String>> = HashMap::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let name = entry
            .map_err(|e| e.to_string())?
            .file_name()
            .into_string()
            .map_err(|_| "目录存在非 UTF-8 名称")?;
        occupied.entry(name.to_lowercase()).or_default().push(name);
    }
    for row in rows {
        let key = row.to_name.to_lowercase();
        if !targets.insert(key.clone()) {
            return Err(format!("目标名称重复：{}", row.to_name));
        }
        if occupied.get(&key).is_some_and(|names| {
            names
                .iter()
                .any(|n| !moving.get(&key).is_some_and(|set| set.contains(n)))
        }) {
            return Err(format!("目标已被未选择的文件或目录占用：{}", row.to_name));
        }
    }
    Ok(())
}
fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
#[cfg(test)]
mod tests;
