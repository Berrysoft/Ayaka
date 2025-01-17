#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use ayaka_runtime::{
    anyhow::{self, anyhow, Result},
    log::{debug, info, warn},
    *,
};
use flexi_logger::{FileSpec, LogSpecification, Logger};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Display};
use tauri::{async_runtime::Mutex, command, AppHandle, Manager, State};

type CommandResult<T> = std::result::Result<T, CommandError>;

#[derive(Debug, Default, Serialize)]
struct CommandError {
    msg: String,
}

impl<E: Into<anyhow::Error>> From<E> for CommandError {
    fn from(e: E) -> Self {
        Self {
            msg: e.into().to_string(),
        }
    }
}

impl Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.msg)
    }
}

#[command]
fn ayaka_version() -> &'static str {
    ayaka_runtime::version()
}

#[derive(Debug, Serialize)]
struct FullSettings {
    settings: Settings,
    contexts: Vec<RawContext>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "t", content = "data")]
enum OpenGameStatus {
    LoadSettings,
    LoadProfile(String),
    CreateRuntime,
    LoadPlugin(String, usize, usize),
    LoadGlobalRecords,
    LoadRecords,
    Loaded,
}

fn emit_open_status(
    handle: &AppHandle,
    status: OpenGameStatus,
) -> std::result::Result<(), tauri::Error> {
    handle.emit_all("ayaka://open_status", status)
}

#[command]
async fn open_game(handle: AppHandle, storage: State<'_, Storage>) -> CommandResult<()> {
    let config = &storage.config;
    let context = Context::open(config, FrontendType::Html);
    pin_mut!(context);
    while let Some(status) = context.next().await {
        match status {
            OpenStatus::LoadProfile => {
                emit_open_status(&handle, OpenGameStatus::LoadProfile(config.clone()))?
            }
            OpenStatus::CreateRuntime => emit_open_status(&handle, OpenGameStatus::CreateRuntime)?,
            OpenStatus::LoadPlugin(name, i, len) => {
                emit_open_status(&handle, OpenGameStatus::LoadPlugin(name, i, len))?
            }
        }
    }
    let mut ctx = context.await?;

    let window = handle.get_window("main").unwrap();
    window.set_title(&ctx.game.title)?;
    let settings = {
        emit_open_status(&handle, OpenGameStatus::LoadSettings)?;
        load_settings(&storage.ident).await.unwrap_or_else(|e| {
            warn!("Load settings failed: {}", e);
            Settings::new()
        })
    };
    ctx.set_settings(settings);

    emit_open_status(&handle, OpenGameStatus::LoadGlobalRecords)?;
    ctx.set_global_record(
        load_global_record(&storage.ident, &ctx.game.title)
            .await
            .unwrap_or_else(|e| {
                warn!("Load global records failed: {}", e);
                Default::default()
            }),
    );

    emit_open_status(&handle, OpenGameStatus::LoadRecords)?;
    *storage.records.lock().await = load_records(&storage.ident, &ctx.game.title)
        .await
        .unwrap_or_else(|e| {
            warn!("Load records failed: {}", e);
            Default::default()
        });
    *storage.context.lock().await = Some(ctx);

    emit_open_status(&handle, OpenGameStatus::Loaded)?;
    Ok(())
}

#[command]
async fn get_settings(storage: State<'_, Storage>) -> CommandResult<Option<Settings>> {
    Ok(storage
        .context
        .lock()
        .await
        .as_ref()
        .map(|ctx| ctx.settings())
        .cloned())
}

#[command]
async fn set_settings(settings: Settings, storage: State<'_, Storage>) -> CommandResult<()> {
    if let Some(context) = storage.context.lock().await.as_mut() {
        context.set_settings(settings);
    }
    Ok(())
}

#[command]
async fn get_records(storage: State<'_, Storage>) -> CommandResult<Vec<ActionRecord>> {
    Ok(storage.records.lock().await.clone())
}

#[command]
async fn save_record_to(index: usize, storage: State<'_, Storage>) -> CommandResult<()> {
    let mut records = storage.records.lock().await;
    if let Some(record) = storage
        .context
        .lock()
        .await
        .as_ref()
        .map(|ctx| ctx.record.clone())
    {
        if index >= records.len() {
            records.push(record);
        } else {
            records[index] = record;
        }
    }
    Ok(())
}

#[command]
async fn save_all(storage: State<'_, Storage>) -> CommandResult<()> {
    if let Some(context) = storage.context.lock().await.as_ref() {
        let game = &context.game.title;
        save_settings(&storage.ident, context.settings()).await?;
        save_global_record(&storage.ident, game, context.global_record()).await?;
        save_records(&storage.ident, game, &storage.records.lock().await).await?;
    }
    Ok(())
}

#[command]
fn choose_locale(locales: Vec<Locale>) -> CommandResult<Option<Locale>> {
    let current = Locale::current();
    debug!("Choose {} from {:?}", current, locales);
    Ok(current.choose_from(&locales).cloned())
}

#[derive(Default)]
struct Storage {
    ident: String,
    config: String,
    records: Mutex<Vec<ActionRecord>>,
    context: Mutex<Option<Context>>,
    action: Mutex<Option<Action>>,
}

impl Storage {
    pub fn new(ident: impl Into<String>, config: impl Into<String>) -> Self {
        Self {
            ident: ident.into(),
            config: config.into(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct GameInfo {
    pub title: String,
    pub author: String,
    pub props: HashMap<String, String>,
}

impl GameInfo {
    pub fn new(game: &Game) -> Self {
        Self {
            title: game.title.clone(),
            author: game.author.clone(),
            props: game.props.clone(),
        }
    }
}

#[command]
async fn info(storage: State<'_, Storage>) -> CommandResult<Option<GameInfo>> {
    let ctx = storage.context.lock().await;
    if let Some(ctx) = ctx.as_ref() {
        Ok(Some(GameInfo::new(&ctx.game)))
    } else {
        warn!("Game hasn't been loaded.");
        Ok(None)
    }
}

#[command]
async fn start_new(locale: Locale, storage: State<'_, Storage>) -> CommandResult<()> {
    if let Some(ctx) = storage.context.lock().await.as_mut() {
        ctx.init_new();
        info!("Init new context with locale {}.", locale);
    } else {
        warn!("Game hasn't been loaded.")
    }
    Ok(())
}

#[command]
async fn start_record(
    locale: Locale,
    index: usize,
    storage: State<'_, Storage>,
) -> CommandResult<()> {
    if let Some(ctx) = storage.context.lock().await.as_mut() {
        let raw_ctx = storage.records.lock().await[index].clone();
        let last_line = raw_ctx.history.last().unwrap();
        *storage.action.lock().await = Some(last_line.clone());
        ctx.init_context(raw_ctx);
        info!("Init new context with locale {}.", locale);
    } else {
        warn!("Game hasn't been loaded.")
    }
    Ok(())
}

#[command]
async fn next_run(storage: State<'_, Storage>) -> CommandResult<bool> {
    let mut context = storage.context.lock().await;
    let action = context.as_mut().and_then(|context| context.next_run());
    if let Some(action) = action {
        debug!("Next action: {:?}", action);
        *storage.action.lock().await = Some(action);
        Ok(true)
    } else {
        debug!("No action left.");
        *storage.action.lock().await = None;
        Ok(false)
    }
}

#[command]
async fn next_back_run(storage: State<'_, Storage>) -> CommandResult<bool> {
    let mut context = storage.context.lock().await;
    let action = context.as_mut().and_then(|context| context.next_back_run());
    if let Some(action) = action {
        debug!("Last action: {:?}", action);
        *storage.action.lock().await = Some(action);
        Ok(true)
    } else {
        debug!("No action in the history.");
        Ok(false)
    }
}

#[command]
async fn current_visited(storage: State<'_, Storage>) -> CommandResult<bool> {
    let action = storage.action.lock().await;
    let visited = if let Some(action) = action.as_ref() {
        let context = storage.context.lock().await;
        context
            .as_ref()
            .map(|context| context.visited(action))
            .unwrap_or_default()
    } else {
        false
    };
    Ok(visited)
}

#[command]
async fn current_run(storage: State<'_, Storage>) -> CommandResult<Option<Action>> {
    Ok(storage.action.lock().await.as_ref().cloned())
}

#[command]
async fn switch(i: usize, storage: State<'_, Storage>) -> CommandResult<RawValue> {
    debug!("Switch {}", i);
    let mut context = storage.context.lock().await;
    let context = context
        .as_mut()
        .ok_or_else(|| anyhow!("Context not initialized."))?;
    let action = storage.action.lock().await;
    let switch = action
        .as_ref()
        .and_then(|action| action.switches.get(i))
        .ok_or_else(|| anyhow!("Index error: {}", i))?;
    Ok(context.call(&switch.action))
}

#[command]
async fn history(storage: State<'_, Storage>) -> CommandResult<Vec<Action>> {
    let mut hs = storage
        .context
        .lock()
        .await
        .as_ref()
        .map(|context| context.record.history.clone())
        .unwrap_or_default();
    hs.reverse();
    debug!("Get history {:?}", hs);
    Ok(hs)
}

fn main() -> Result<()> {
    let port =
        portpicker::pick_unused_port().ok_or_else(|| anyhow!("failed to find unused port"))?;
    info!("Picked port {}", port);
    tauri::Builder::default()
        .plugin(tauri_plugin_localhost::Builder::new(port).build())
        .setup(|app| {
            let ident = app.config().tauri.bundle.identifier.clone();
            let log_handle = if cfg!(debug_assertions) {
                Logger::with(LogSpecification::parse("warn,ayaka=debug,ayalog=debug")?)
                    .log_to_stdout()
                    .set_palette("b1;3;2;4;6".to_string())
                    .use_utc()
                    .start()?
            } else {
                Logger::with(LogSpecification::parse("info,wasmer=warn")?)
                    .log_to_file(
                        FileSpec::default()
                            .directory(app.path_resolver().log_dir().unwrap())
                            .basename("ayaka-gui"),
                    )
                    .use_utc()
                    .start()?
            };
            app.manage(log_handle);
            #[cfg(debug_assertions)]
            {
                let window = app.get_window("main").unwrap();
                window.open_devtools();
            }
            let matches = app.get_cli_matches()?;
            let config = matches.args["config"]
                .value
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    std::env::current_exe()
                        .unwrap()
                        .parent()
                        .unwrap()
                        .join("config.yaml")
                        .to_string_lossy()
                        .into_owned()
                });
            app.manage(Storage::new(ident, config));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ayaka_version,
            open_game,
            get_settings,
            set_settings,
            get_records,
            save_record_to,
            save_all,
            choose_locale,
            info,
            start_new,
            start_record,
            next_run,
            next_back_run,
            current_run,
            current_visited,
            switch,
            history,
        ])
        .run(tauri::generate_context!())?;
    Ok(())
}
