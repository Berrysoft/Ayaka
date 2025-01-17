#![feature(once_cell)]

use ayaka_bindings::*;
use log::error;
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::sync::{LazyLock, Mutex};

#[export]
fn plugin_type() -> PluginType {
    PluginType::default()
}

static RNG: LazyLock<Mutex<StdRng>> = LazyLock::new(|| Mutex::new(StdRng::from_entropy()));

#[export]
fn rnd(args: Vec<RawValue>) -> RawValue {
    if let Ok(mut rng) = RNG.lock() {
        let res = match args.len() {
            0 => rng.gen(),
            1 => rng.gen_range(0..args[0].get_num()),
            _ => rng.gen_range(args[0].get_num()..args[1].get_num()),
        };
        RawValue::Num(res)
    } else {
        error!("Cannot get random engine.");
        RawValue::Unit
    }
}
