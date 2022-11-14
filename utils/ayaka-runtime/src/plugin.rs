//! The plugin utilities.

#![allow(unsafe_code)]

use crate::*;
use anyhow::Result;
use ayaka_bindings_types::*;
use log::warn;
use scopeguard::defer;
use serde::{de::DeserializeOwned, Serialize};
use std::{cell::RefCell, collections::HashMap, marker::Tuple, ops::Deref, path::Path, sync::Arc};
use stream_future::stream;
use tryiterator::TryIteratorExt;
use wasmi::{core::Trap, *};
use wasmi_wasi::*;

#[derive(Clone)]
struct HostStore(Arc<RefCell<Store<WasiCtx>>>);

// TODO: Make sure it is safe
unsafe impl Send for HostStore {}
unsafe impl Sync for HostStore {}

impl Deref for HostStore {
    type Target = RefCell<Store<WasiCtx>>;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

/// An instance of a WASM plugin module.
pub struct Host {
    instance: Instance,
    memory: Memory,
    abi_free: TypedFunc<(i32, i32), ()>,
    abi_alloc: TypedFunc<i32, i32>,
}

unsafe fn mem_slice<'a, T: 'a>(
    store: impl Into<StoreContext<'a, T>> + 'a,
    memory: &Memory,
    start: i32,
    len: i32,
) -> &'a [u8] {
    memory
        .data(store)
        .get_unchecked(start as usize..)
        .get_unchecked(..len as usize)
}

unsafe fn mem_slice_mut<'a, T: 'a>(
    store: impl Into<StoreContextMut<'a, T>> + 'a,
    memory: &Memory,
    start: i32,
    len: i32,
) -> &'a mut [u8] {
    memory
        .data_mut(store)
        .get_unchecked_mut(start as usize..)
        .get_unchecked_mut(..len as usize)
}

impl Host {
    /// Loads the WASM [`Module`], with some imports.
    pub fn new<T>(
        mut store: StoreContextMut<T>,
        module: &Module,
        linker: &Linker<WasiCtx>,
    ) -> Result<Self> {
        let instance = linker
            .instantiate(store.as_context_mut(), module)?
            .start(store.as_context_mut())?;
        let memory = instance
            .get_export(store.as_context(), "memory")
            .unwrap()
            .into_memory()
            .unwrap();
        let abi_free = instance
            .get_export(store.as_context(), "__abi_free")
            .unwrap()
            .into_func()
            .unwrap()
            .typed(store.as_context())?;
        let abi_alloc = instance
            .get_export(store.as_context(), "__abi_alloc")
            .unwrap()
            .into_func()
            .unwrap()
            .typed(store.as_context())?;
        Ok(Self {
            instance,
            memory,
            abi_free,
            abi_alloc,
        })
    }

    /// Calls a method by name.
    ///
    /// The args and returns are passed by MessagePack with [`rmp_serde`].
    pub fn call<T, Params: Serialize, Res: DeserializeOwned>(
        &self,
        mut store: StoreContextMut<T>,
        name: &str,
        args: Params,
    ) -> Result<Res> {
        let memory = &self.memory;
        let func = self
            .instance
            .get_export(store.as_context(), name)
            .unwrap()
            .into_func()
            .unwrap()
            .typed::<(i32, i32), i64>(store.as_context())?;

        let data = rmp_serde::to_vec(&args)?;

        let ptr = self
            .abi_alloc
            .call(store.as_context_mut(), data.len() as i32)?;
        //defer! { self.abi_free.call(store, (ptr, data.len() as i32)).unwrap(); }
        unsafe { mem_slice_mut(store.as_context_mut(), memory, ptr, data.len() as i32) }
            .copy_from_slice(&data);

        let res = func.call(store.as_context_mut(), (data.len() as i32, ptr))?;
        let (len, res) = ((res >> 32) as i32, (res & 0xFFFFFFFF) as i32);
        //defer! { self.abi_free.call(store, (res, len)).unwrap(); }

        let res_data = unsafe { mem_slice(store.as_context(), memory, res, len) };
        let res_data = rmp_serde::from_slice(res_data)?;
        Ok(res_data)
    }
}

pub struct HostRef<'a> {
    store: HostStore,
    host: &'a Host,
}

impl<'a> HostRef<'a> {
    fn new(store: HostStore, host: &'a Host) -> Self {
        Self { store, host }
    }

    fn call<Params: Serialize, Res: DeserializeOwned>(
        &self,
        name: &str,
        args: Params,
    ) -> Result<Res> {
        self.host
            .call(self.store.borrow_mut().as_context_mut(), name, args)
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
    pub fn process_action(&self, ctx: ActionProcessContextRef) -> Result<ActionProcessResult> {
        self.call("process_action", (ctx,))
    }

    /// Calls a custom command in the text plugin.
    pub fn dispatch_text(
        &self,
        name: &str,
        args: &[String],
        ctx: TextProcessContextRef,
    ) -> Result<TextProcessResult> {
        self.call(name, (args, ctx))
    }

    /// Calls a custom command in the line plugin.
    pub fn dispatch_line(
        &self,
        name: &str,
        ctx: LineProcessContextRef,
    ) -> Result<LineProcessResult> {
        self.call(name, (ctx,))
    }

    /// Processes [`Game`] when opening the config file.
    pub fn process_game(&self, ctx: GameProcessContextRef) -> Result<GameProcessResult> {
        self.call("process_game", (ctx,))
    }
}

/// The plugin runtime.
pub struct Runtime {
    store: HostStore,
    /// The plugins map by name.
    modules: HashMap<String, Host>,
    /// The action plugins.
    action_modules: Vec<String>,
    /// The text plugins by command name.
    text_modules: HashMap<String, String>,
    /// The line plugins by command name.
    line_modules: HashMap<String, String>,
    /// The game plugins.
    game_modules: Vec<String>,
}

/// The load status of [`Runtime`].
#[derive(Debug, Clone)]
pub enum LoadStatus {
    /// Start creating the engine.
    CreateEngine,
    /// Loading the plugin.
    LoadPlugin(String, usize, usize),
}

unsafe fn import<T, Params: DeserializeOwned + Tuple, Res: Serialize>(
    mut store: Caller<T>,
    len: i32,
    data: i32,
    f: impl FnOnce<Params, Output = Res>,
) -> std::result::Result<u64, Trap> {
    let memory = store.get_export("memory").unwrap().into_memory().unwrap();
    let data = mem_slice(&store, &memory, data, len);
    let data = rmp_serde::from_slice(data).map_err(|e| Trap::new(e.to_string()))?;
    let res = f.call_once(data);
    let data = rmp_serde::to_vec(&res).map_err(|e| Trap::new(e.to_string()))?;
    let alloc = store
        .get_export("__abi_alloc")
        .unwrap()
        .into_func()
        .unwrap()
        .typed::<i32, i32>(&store)
        .map_err(|e| Trap::new(e.to_string()))?;
    let ptr = alloc.call(&mut store, data.len() as _)?;
    mem_slice_mut(&mut store, &memory, ptr, data.len() as _).copy_from_slice(&data);
    Ok(((data.len() as u64) << 32) | (ptr as u64))
}

impl Runtime {
    fn new_linker() -> Result<(Linker<WasiCtx>, HostStore)> {
        let engine = Engine::default();
        let wasi = WasiCtxBuilder::new().inherit_stdio().build();
        let mut store = Store::new(&engine, wasi);
        let mut linker = Linker::new();
        define_wasi(&mut linker, &mut store, |ctx| ctx)?;

        let log_func = Func::wrap(
            &mut store,
            |store: Caller<WasiCtx>, len: i32, data: i32| unsafe {
                import(store, len, data, |data: Record| {
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
                })
            },
        );
        let log_flush_func = Func::wrap(&mut store, || log::logger().flush());

        linker.define("log", "__log", log_func)?;
        linker.define("log", "__log_flush", log_flush_func)?;

        Ok((linker, HostStore(Arc::new(RefCell::new(store)))))
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
        let (linker, store) = Self::new_linker()?;
        let mut modules = HashMap::new();
        let mut action_modules = vec![];
        let mut text_modules = HashMap::new();
        let mut line_modules = HashMap::new();
        let mut game_modules = vec![];
        let paths = if names.is_empty() {
            std::fs::read_dir(path)?
                .try_filter_map(|f| {
                    let p = f.path();
                    if p.is_file() && p.extension().unwrap_or_default() == "wasm" {
                        let name = p
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();
                        Ok(Some((name, p)))
                    } else {
                        Ok(None)
                    }
                })
                .try_collect::<Vec<_>>()?
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
            let buf = std::fs::read(p)?;
            let module = Module::new(store.borrow().engine(), buf.as_slice())?;
            let runtime = Host::new(store.borrow_mut().as_context_mut(), &module, &linker)?;
            let runtime_ref = HostRef::new(store.clone(), &runtime);
            let plugin_type = runtime_ref.plugin_type()?;
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
            for cmd in plugin_type.line {
                let res = line_modules.insert(cmd.clone(), name.clone());
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
            store,
            modules,
            action_modules,
            text_modules,
            line_modules,
            game_modules,
        })
    }

    pub fn module(&self, key: &str) -> Option<HostRef> {
        self.modules
            .get(key)
            .map(|host| HostRef::new(self.store.clone(), host))
    }

    pub fn action_modules(&self) -> impl Iterator<Item = HostRef> {
        ModuleIter::new(self.store.clone(), &self.modules, &self.action_modules)
    }

    pub fn text_module(&self, cmd: &str) -> Option<HostRef> {
        self.text_modules.get(cmd).and_then(|key| self.module(key))
    }

    pub fn line_module(&self, cmd: &str) -> Option<HostRef> {
        self.line_modules.get(cmd).and_then(|key| self.module(key))
    }

    pub fn game_modules(&self) -> impl Iterator<Item = HostRef> {
        ModuleIter::new(self.store.clone(), &self.modules, &self.game_modules)
    }
}

struct ModuleIter<'a> {
    store: HostStore,
    modules: &'a HashMap<String, Host>,
    keys: &'a [String],
    index: usize,
}

impl<'a> ModuleIter<'a> {
    pub fn new(store: HostStore, modules: &'a HashMap<String, Host>, keys: &'a [String]) -> Self {
        Self {
            store,
            modules,
            keys,
            index: 0,
        }
    }
}

impl<'a> Iterator for ModuleIter<'a> {
    type Item = HostRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let i = self.index;
        if i < self.keys.len() {
            self.index += 1;
            Some(HostRef::new(
                self.store.clone(),
                &self.modules[&self.keys[i]],
            ))
        } else {
            None
        }
    }
}
