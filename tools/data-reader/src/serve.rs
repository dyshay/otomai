//! Web server for browsing and editing Dofus data files.

use crate::{d2i, d2o, d2o_writer, d2p};
use anyhow::Result;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const FRONTEND_HTML: &str = include_str!("frontend/index.html");

struct AppState {
    data_dir: PathBuf,
    d2o_cache: Mutex<HashMap<String, d2o::D2OReader>>,
    d2o_edits: Mutex<HashMap<String, HashMap<i32, Value>>>,
}

pub async fn run(data_dir: PathBuf, port: u16) -> Result<()> {
    let state = Arc::new(AppState {
        data_dir: data_dir.clone(),
        d2o_cache: Mutex::new(HashMap::new()),
        d2o_edits: Mutex::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/", get(index_html))
        .route("/api/files", get(list_files))
        .route("/api/d2o/list", get(list_d2o_objects))
        .route("/api/d2o/schema", get(get_d2o_schema))
        .route("/api/d2o/object", get(get_d2o_object))
        .route("/api/d2o/update", put(update_d2o_object))
        .route("/api/d2o/save", post(save_d2o_file))
        .route("/api/d2i/list", get(list_d2i_texts))
        .route("/api/d2p/list", get(list_d2p_entries))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    eprintln!("Otomai Data Editor");
    eprintln!("  Dossier : {}", data_dir.display());
    eprintln!("  URL     : http://localhost:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn index_html() -> Html<&'static str> {
    Html(FRONTEND_HTML)
}

async fn list_files(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut d2o = Vec::new();
    let mut d2i = Vec::new();
    let mut d2p = Vec::new();

    if let Ok(entries) = scan_files(&state.data_dir) {
        for (name, ext) in entries {
            match ext.as_str() {
                "d2o" => d2o.push(name),
                "d2i" => d2i.push(name),
                "d2p" => d2p.push(name),
                _ => {}
            }
        }
    }

    d2o.sort();
    d2i.sort();
    d2p.sort();

    Json(json!({ "d2o": d2o, "d2i": d2i, "d2p": d2p }))
}

fn scan_files(dir: &std::path::Path) -> Result<Vec<(String, String)>> {
    let mut files = Vec::new();
    scan_dir_recursive(dir, dir, &mut files)?;
    Ok(files)
}

fn scan_dir_recursive(
    base: &std::path::Path,
    dir: &std::path::Path,
    files: &mut Vec<(String, String)>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            scan_dir_recursive(base, &path, files)?;
        } else if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy().to_string();
            if matches!(ext_str.as_str(), "d2o" | "d2i" | "d2p") {
                let rel = path.strip_prefix(base).unwrap_or(&path);
                files.push((rel.to_string_lossy().to_string(), ext_str));
            }
        }
    }
    Ok(())
}

fn ensure_d2o_loaded(state: &AppState, file: &str) -> Result<(), (StatusCode, String)> {
    let mut cache = state.d2o_cache.lock().unwrap();
    if !cache.contains_key(file) {
        let path = state.data_dir.join(file);
        let reader = d2o::D2OReader::open(&path)
            .map_err(|e| (StatusCode::NOT_FOUND, format!("Cannot open {}: {}", file, e)))?;
        cache.insert(file.to_string(), reader);
    }
    Ok(())
}

// --- Query param structs ---

#[derive(Deserialize)]
struct FileParams {
    file: String,
    page: Option<usize>,
    per_page: Option<usize>,
    search: Option<String>,
}

#[derive(Deserialize)]
struct ObjectParams {
    file: String,
    id: i32,
}

// --- D2O handlers ---

async fn list_d2o_objects(
    State(state): State<Arc<AppState>>,
    Query(params): Query<FileParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let file = &params.file;
    ensure_d2o_loaded(&state, file)?;
    let cache = state.d2o_cache.lock().unwrap();
    let reader = cache.get(file).unwrap();
    let edits = state.d2o_edits.lock().unwrap();
    let file_edits = edits.get(file);

    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(50).min(500);
    let search = params.search.unwrap_or_default().to_lowercase();

    let ids = reader.object_ids();
    let mut items: Vec<Value> = Vec::new();

    for id in &ids {
        let obj = match file_edits.and_then(|e| e.get(id)) {
            Some(edited) => edited.clone(),
            None => reader.read_object(*id).map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Read error: {}", e))
            })?,
        };

        let class_name = obj.get("_class").and_then(|v| v.as_str()).unwrap_or("");
        let edited = file_edits.is_some_and(|e| e.contains_key(id));
        let preview = obj.get("nameId").or(obj.get("name")).or(obj.get("id"));

        if !search.is_empty() {
            let obj_str = serde_json::to_string(&obj).unwrap_or_default().to_lowercase();
            if !obj_str.contains(&search) {
                continue;
            }
        }

        items.push(json!({
            "id": id, "_class": class_name, "_edited": edited, "_preview": preview,
        }));
    }

    let total = items.len();
    let start = (page - 1) * per_page;
    let page_items: Vec<_> = items.into_iter().skip(start).take(per_page).collect();

    Ok(Json(json!({
        "items": page_items,
        "total": total,
        "page": page,
        "per_page": per_page,
        "total_pages": (total + per_page - 1) / per_page,
    })))
}

async fn get_d2o_schema(
    State(state): State<Arc<AppState>>,
    Query(params): Query<FileParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let file = &params.file;
    ensure_d2o_loaded(&state, file)?;
    let cache = state.d2o_cache.lock().unwrap();
    let reader = cache.get(file).unwrap();

    let classes: Vec<_> = reader.classes().values().map(|c| {
        json!({
            "id": c.class_id,
            "name": &c.name,
            "package": &c.package,
            "fields": c.fields.iter().map(|f| json!({
                "name": &f.name,
                "type": format!("{:?}", f.field_type),
            })).collect::<Vec<_>>(),
        })
    }).collect();

    Ok(Json(json!({ "classes": classes })))
}

async fn get_d2o_object(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ObjectParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let file = &params.file;
    let id = params.id;

    {
        let edits = state.d2o_edits.lock().unwrap();
        if let Some(obj) = edits.get(file).and_then(|e| e.get(&id)) {
            return Ok(Json(json!({ "object": obj, "edited": true })));
        }
    }

    ensure_d2o_loaded(&state, file)?;
    let cache = state.d2o_cache.lock().unwrap();
    let reader = cache.get(file).unwrap();

    let obj = reader.read_object(id).map_err(|e| {
        (StatusCode::NOT_FOUND, format!("Object {} not found: {}", id, e))
    })?;

    Ok(Json(json!({ "object": obj, "edited": false })))
}

async fn update_d2o_object(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ObjectParams>,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let obj = body.get("object").cloned().ok_or_else(|| {
        (StatusCode::BAD_REQUEST, "Missing 'object' field".to_string())
    })?;

    let mut edits = state.d2o_edits.lock().unwrap();
    let file_edits = edits.entry(params.file.clone()).or_default();
    file_edits.insert(params.id, obj);

    Ok(Json(json!({ "ok": true, "pending_edits": file_edits.len() })))
}

async fn save_d2o_file(
    State(state): State<Arc<AppState>>,
    Query(params): Query<FileParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let file = &params.file;
    ensure_d2o_loaded(&state, file)?;

    let (classes, objects) = {
        let cache = state.d2o_cache.lock().unwrap();
        let reader = cache.get(file).unwrap();
        let edits = state.d2o_edits.lock().unwrap();
        let file_edits = edits.get(file);
        let ids = reader.object_ids();

        let mut objects = Vec::with_capacity(ids.len());
        for id in &ids {
            let obj = match file_edits.and_then(|e| e.get(id)) {
                Some(edited) => edited.clone(),
                None => reader.read_object(*id).map_err(|e| {
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("Read error: {}", e))
                })?,
            };
            objects.push((*id, obj));
        }
        (reader.classes().clone(), objects)
    };

    let bytes = d2o_writer::write_d2o(&classes, &objects).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Write error: {}", e))
    })?;

    let path = state.data_dir.join(file);
    let backup_path = path.with_extension("d2o.bak");
    std::fs::copy(&path, &backup_path).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Backup error: {}", e))
    })?;
    std::fs::write(&path, &bytes).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Write error: {}", e))
    })?;

    // Reload cache and clear edits
    { state.d2o_cache.lock().unwrap().remove(file); }
    { state.d2o_edits.lock().unwrap().remove(file); }

    Ok(Json(json!({
        "ok": true,
        "bytes_written": bytes.len(),
        "backup": backup_path.display().to_string(),
    })))
}

// --- D2I handler ---

async fn list_d2i_texts(
    State(state): State<Arc<AppState>>,
    Query(params): Query<FileParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let path = state.data_dir.join(&params.file);
    let reader = d2i::D2IReader::open(&path)
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Cannot open: {}", e)))?;

    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(50).min(500);
    let search = params.search.unwrap_or_default().to_lowercase();

    let all = reader.all_texts().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Read error: {}", e))
    })?;

    let mut entries: Vec<_> = all.into_iter().collect();
    entries.sort_by_key(|(id, _)| *id);

    if !search.is_empty() {
        entries.retain(|(id, text)| {
            text.to_lowercase().contains(&search) || id.to_string().contains(&search)
        });
    }

    let total = entries.len();
    let start = (page - 1) * per_page;
    let page_items: Vec<_> = entries
        .into_iter()
        .skip(start)
        .take(per_page)
        .map(|(id, text)| json!({ "id": id, "text": text }))
        .collect();

    Ok(Json(json!({
        "items": page_items,
        "total": total,
        "page": page,
        "per_page": per_page,
        "total_pages": (total + per_page - 1) / per_page,
    })))
}

// --- D2P handler ---

async fn list_d2p_entries(
    State(state): State<Arc<AppState>>,
    Query(params): Query<FileParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let path = state.data_dir.join(&params.file);
    let reader = d2p::D2PReader::open(&path)
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Cannot open: {}", e)))?;

    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(100).min(1000);
    let search = params.search.unwrap_or_default().to_lowercase();

    let mut files: Vec<String> = reader.filenames().into_iter().map(|s| s.to_string()).collect();
    if !search.is_empty() {
        files.retain(|f| f.to_lowercase().contains(&search));
    }

    let total = files.len();
    let start = (page - 1) * per_page;
    let page_items: Vec<_> = files.into_iter().skip(start).take(per_page).collect();

    Ok(Json(json!({
        "items": page_items,
        "total": total,
        "page": page,
        "per_page": per_page,
        "total_pages": (total + per_page - 1) / per_page,
        "properties": reader.properties(),
    })))
}
