use gal_script::RawValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct Game {
    pub title: String,
    pub author: String,
    pub paras: Vec<Paragraph>,
    pub plugins: PathBuf,
}

impl Game {
    pub fn find_para(&self, tag: &str) -> Option<&Paragraph> {
        for p in &self.paras {
            if p.tag == tag {
                return Some(p);
            }
        }
        None
    }
}

#[derive(Debug, Deserialize)]
pub struct Paragraph {
    pub tag: String,
    pub title: String,
    pub actions: Vec<String>,
    pub next: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct RawContext {
    pub cur_para: String,
    pub cur_act: usize,
    pub locals: VarMap,
}

pub type VarMap = HashMap<String, RawValue>;
