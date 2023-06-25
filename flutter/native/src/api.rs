use ayaka_model::{
    anyhow::{anyhow, Error, Result},
    SettingsManager,
};
use flutter_rust_bridge::RustOpaque;
use serde::{de::DeserializeOwned, Serialize};

pub use ayaka_model::GameViewModel;
pub use ayaka_plugin_wasmi::WasmiModule;
pub use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

pub struct FlutterSettingsManager {
    pub local_data_dir: RustOpaque<PathBuf>,
    pub config_dir: RustOpaque<PathBuf>,
}

impl FlutterSettingsManager {
    pub fn new() -> Result<FlutterSettingsManager> {
        Ok(Self::new_with_dirs(
            dirs_next::data_local_dir()
                .ok_or_else(|| anyhow!("cannot get local data dir"))?
                .join("com.unigal.ayaka")
                .to_string_lossy()
                .into_owned(),
            dirs_next::config_dir()
                .ok_or_else(|| anyhow!("cannot get config dir"))?
                .join("com.unigal.ayaka")
                .to_string_lossy()
                .into_owned(),
        ))
    }

    pub fn new_with_dirs(local_data_dir: String, config_dir: String) -> FlutterSettingsManager {
        Self {
            local_data_dir: RustOpaque::new(local_data_dir.into()),
            config_dir: RustOpaque::new(config_dir.into()),
        }
    }

    fn records_path_root(&self, game: &str) -> PathBuf {
        self.local_data_dir.join("save").join(game)
    }
}

impl SettingsManager for FlutterSettingsManager {
    fn load_file<T: DeserializeOwned>(&self, path: impl AsRef<Path>) -> Result<T> {
        let file = std::fs::File::open(path)?;
        Ok(serde_json::from_reader(file)?)
    }

    fn save_file<T: Serialize>(
        &self,
        path: impl AsRef<Path>,
        data: &T,
        pretty: bool,
    ) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let output = std::fs::File::create(path)?;
        if pretty {
            serde_json::to_writer_pretty(output, data)
        } else {
            serde_json::to_writer(output, data)
        }?;
        Ok(())
    }

    fn settings_path(&self) -> Result<PathBuf> {
        Ok(self.config_dir.join("settings.json"))
    }

    fn global_record_path(&self, game: &str) -> Result<PathBuf> {
        Ok(self.records_path_root(game).join("global.json"))
    }

    fn records_path(&self, game: &str) -> Result<impl Iterator<Item = Result<PathBuf>>> {
        let ctx_path = self.records_path_root(game);
        Ok(std::fs::read_dir(ctx_path)?.filter_map(|entry| {
            entry
                .map_err(Error::from)
                .map(|entry| {
                    let p = entry.path();
                    if p.is_file() && p.file_name().unwrap_or_default() != "global.json" {
                        Some(p)
                    } else {
                        None
                    }
                })
                .transpose()
        }))
    }

    fn record_path(&self, game: &str, i: usize) -> Result<PathBuf> {
        Ok(self
            .records_path_root(game)
            .join(i.to_string())
            .with_extension("json"))
    }
}

pub struct Runtime {
    pub model: RustOpaque<Mutex<GameViewModel<FlutterSettingsManager, WasmiModule>>>,
}

impl Runtime {
    pub fn new(s: FlutterSettingsManager) -> Runtime {
        Self {
            model: RustOpaque::new(Mutex::new(GameViewModel::new(s))),
        }
    }
}
