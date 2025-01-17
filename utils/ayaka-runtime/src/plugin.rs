//! The plugin utilities.

#![allow(unsafe_code)]
#![allow(clippy::mut_from_ref)]

use crate::*;
use anyhow::Result;
use ayaka_bindings_types::*;
use futures_util::TryStreamExt;
use log::warn;
use scopeguard::defer;
use serde::{de::DeserializeOwned, Serialize};
use std::{collections::HashMap, path::Path};
use stream_future::stream;
use tokio_stream::wrappers::ReadDirStream;
use wasmer::*;
use wasmer_wasi::*;

/// An instance of a WASM plugin module.
pub struct Host {
    abi_free: NativeFunc<(i32, i32), ()>,
    abi_alloc: NativeFunc<i32, i32>,
    instance: Instance,
}

unsafe fn mem_slice(memory: &Memory, start: i32, len: i32) -> &[u8] {
    memory
        .data_unchecked()
        .get_unchecked(start as usize..)
        .get_unchecked(..len as usize)
}

unsafe fn mem_slice_mut(memory: &Memory, start: i32, len: i32) -> &mut [u8] {
    memory
        .data_unchecked_mut()
        .get_unchecked_mut(start as usize..)
        .get_unchecked_mut(..len as usize)
}

impl Host {
    /// Loads the WASM [`Module`], with some imports.
    pub fn new(module: &Module, resolver: &(dyn Resolver + Send + Sync)) -> Result<Self> {
        let instance = Instance::new(module, resolver)?;
        let abi_free = instance.exports.get_native_function("__abi_free")?;
        let abi_alloc = instance.exports.get_native_function("__abi_alloc")?;
        Ok(Self {
            abi_free,
            abi_alloc,
            instance,
        })
    }

    /// Calls a method by name.
    ///
    /// The args and returns are passed by MessagePack with [`rmp_serde`].
    pub fn call<Params: Serialize, Res: DeserializeOwned>(
        &self,
        name: &str,
        args: Params,
    ) -> Result<Res> {
        let memory = self.instance.exports.get_memory("memory")?;
        let func = self
            .instance
            .exports
            .get_native_function::<(i32, i32), u64>(name)?;

        let data = rmp_serde::to_vec(&args)?;

        let ptr = self.abi_alloc.call(data.len() as i32)?;
        defer! { self.abi_free.call(ptr, data.len() as i32).unwrap(); }
        unsafe { mem_slice_mut(memory, ptr, data.len() as i32) }.copy_from_slice(&data);

        let res = func.call(data.len() as i32, ptr)?;
        let (len, res) = ((res >> 32) as i32, (res & 0xFFFFFFFF) as i32);
        defer! { self.abi_free.call(res, len).unwrap(); }

        let res_data = unsafe { mem_slice(memory, res, len) };
        let res_data = rmp_serde::from_slice(res_data)?;
        Ok(res_data)
    }

    /// Calls a script plugin method by name.
    pub fn dispatch_method(&self, name: &str, args: &[RawValue]) -> Result<RawValue> {
        self.call(name, (args,))
    }

    /// Gets the [`PluginType`].
    pub fn plugin_type(&self) -> Result<PluginType> {
        self.call("plugin_type", ())
    }

    /// Processes [`Action`] in action plugin.
    pub fn process_action(&self, ctx: ActionProcessContextRef) -> Result<Action> {
        self.call("process_action", (ctx,))
    }

    /// Gets registered TeX commands of a text plugin.
    pub fn text_commands(&self) -> Result<Vec<String>> {
        self.call("text_commands", ())
    }

    /// Calls a custom command in the text plugin.
    pub fn dispatch_command(
        &self,
        name: &str,
        args: &[String],
        ctx: TextProcessContextRef,
    ) -> Result<TextProcessResult> {
        self.call(name, (args, ctx))
    }

    /// Processes [`Game`] when opening the config file.
    pub fn process_game(&self, ctx: GameProcessContextRef) -> Result<GameProcessResult> {
        self.call("process_game", (ctx,))
    }
}

/// The plugin runtime.
pub struct Runtime {
    /// The plugins map by name.
    pub modules: HashMap<String, Host>,
    /// The action plugins.
    pub action_modules: Vec<String>,
    /// The text plugins by command name.
    pub text_modules: HashMap<String, String>,
    /// The game plugins.
    pub game_modules: Vec<String>,
}

/// The load status of [`Runtime`].
#[derive(Debug, Clone)]
pub enum LoadStatus {
    /// Start creating the engine.
    CreateEngine,
    /// Loading the plugin.
    LoadPlugin(String, usize, usize),
}

#[derive(Default, Clone, WasmerEnv)]
struct RuntimeInstanceData {
    #[wasmer(export)]
    memory: LazyInit<Memory>,
}

impl Runtime {
    fn imports(store: &Store) -> Result<Box<dyn NamedResolver + Send + Sync>> {
        let log_func = Function::new_native_with_env(
            store,
            RuntimeInstanceData::default(),
            |env_data: &RuntimeInstanceData, len: i32, data: i32| {
                let memory = unsafe { env_data.memory.get_unchecked() };
                let data = unsafe { mem_slice(memory, data, len) };
                let data: Record = rmp_serde::from_slice(data).unwrap();
                log::logger().log(
                    &log::Record::builder()
                        .level(data.level)
                        .target(&data.target)
                        .args(format_args!("{}", data.msg))
                        .module_path(data.module_path.as_deref())
                        .file(data.file.as_deref())
                        .line(data.line)
                        .build(),
                );
            },
        );
        let log_flush_func = Function::new_native(store, || log::logger().flush());
        let import_object = imports! {
            "log" => {
                "__log" => log_func,
                "__log_flush" => log_flush_func,
            }
        };
        let wasi_env = WasiState::new("ayaka-runtime")
            .preopen_dir("/")?
            .finalize()?;
        let wasi_import = generate_import_object_from_env(store, wasi_env, WasiVersion::Latest);
        Ok(Box::new(import_object.chain_front(wasi_import)))
    }

    /// Load plugins from specific directory and plugin names.
    ///
    /// The actual load folder will be `rel_to.join(dir)`.
    ///
    /// If `names` is empty, all WASM files will be loaded.
    #[stream(LoadStatus, lifetime = "'a")]
    pub async fn load<'a>(
        dir: impl AsRef<Path> + 'a,
        rel_to: impl AsRef<Path> + 'a,
        names: &'a [impl AsRef<str>],
    ) -> Result<Self> {
        let path = rel_to.as_ref().join(dir);
        yield LoadStatus::CreateEngine;
        let store = Store::default();
        let import_object = Self::imports(&store)?;
        let mut modules = HashMap::new();
        let mut action_modules = vec![];
        let mut text_modules = HashMap::new();
        let mut game_modules = vec![];
        let paths = if names.is_empty() {
            ReadDirStream::new(tokio::fs::read_dir(path).await?)
                .try_filter_map(|f| async move {
                    let p = f.path();
                    if p.extension()
                        .map(|s| s.to_string_lossy())
                        .unwrap_or_default()
                        == "wasm"
                    {
                        let name = p
                            .file_stem()
                            .map(|s| s.to_string_lossy())
                            .unwrap_or_default()
                            .into_owned();
                        Ok(Some((name, p)))
                    } else {
                        Ok(None)
                    }
                })
                .try_collect::<Vec<_>>()
                .await?
        } else {
            names
                .iter()
                .filter_map(|name| {
                    let name = name.as_ref();
                    let p = path.join(name).with_extension("wasm");
                    if p.exists() {
                        Some((name.to_string(), p))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };
        let total_len = paths.len();
        for (i, (name, p)) in paths.into_iter().enumerate() {
            yield LoadStatus::LoadPlugin(name.clone(), i, total_len);
            let buf = tokio::fs::read(p).await?;
            let module = Module::from_binary(&store, &buf)?;
            let runtime = Host::new(&module, &import_object)?;
            let plugin_type = runtime.plugin_type()?;
            if plugin_type.action {
                action_modules.push(name.clone());
            }
            for cmd in plugin_type.text {
                let res = text_modules.insert(cmd.clone(), name.clone());
                if let Some(old_module) = res {
                    warn!(
                        "Command `{}` is overrided by \"{}\" over \"{}\"",
                        cmd, name, old_module
                    );
                }
            }
            if plugin_type.game {
                game_modules.push(name.clone());
            }
            modules.insert(name, runtime);
        }
        Ok(Self {
            modules,
            action_modules,
            text_modules,
            game_modules,
        })
    }
}
