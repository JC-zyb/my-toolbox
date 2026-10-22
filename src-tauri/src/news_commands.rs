use crate::{
    desktop::AppState,
    news::{NewsSnapshot, TranslationResult},
};
use tauri::State;

#[tauri::command]
pub fn get_agent_news(state: State<AppState>) -> Result<NewsSnapshot, String> {
    state.news.snapshot()
}

#[tauri::command]
pub async fn refresh_agent_news(
    force: bool,
    state: State<'_, AppState>,
) -> Result<NewsSnapshot, String> {
    state.news.refresh(force).await
}

#[tauri::command]
pub async fn set_agent_news_preferences(
    automatic: bool,
    enabled: Vec<String>,
    state: State<'_, AppState>,
) -> Result<NewsSnapshot, String> {
    state.news.preferences(automatic, enabled).await
}

#[tauri::command]
pub async fn set_agent_news_language(
    language: String,
    state: State<'_, AppState>,
) -> Result<NewsSnapshot, String> {
    state.news.set_language(language).await
}

#[tauri::command]
pub async fn translate_agent_news(
    urls: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<TranslationResult>, String> {
    state.news.translate(urls).await
}

#[tauri::command]
pub async fn add_agent_news_source(
    name: String,
    url: String,
    group: String,
    state: State<'_, AppState>,
) -> Result<NewsSnapshot, String> {
    state.news.add_source(name, url, group).await
}

#[tauri::command]
pub async fn remove_agent_news_source(
    id: String,
    state: State<'_, AppState>,
) -> Result<NewsSnapshot, String> {
    state.news.remove_source(id).await
}

#[tauri::command]
pub async fn preview_agent_news(
    url: String,
    state: State<'_, AppState>,
) -> Result<crate::news::NewsPreview, String> {
    state.news.preview(url).await
}
