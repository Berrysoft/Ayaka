#[doc(no_inline)]
pub use ayaka_bindings_types::{Action, Switch};
#[doc(no_inline)]
pub use fallback::Fallback;

use crate::*;
use serde::Deserialize;
use std::{collections::HashMap, path::PathBuf};

/// The paragraph in a game config.
#[derive(Debug, Deserialize)]
pub struct Paragraph {
    /// The tag and key of a paragraph.
    /// They are referenced in `next`.
    pub tag: String,
    /// The title of a paragraph.
    /// It can be [`None`], but better with a human-readable one.
    pub title: Option<String>,
    /// The texts.
    /// They will be parsed into [`ayaka_script::Text`] later.
    pub texts: Vec<String>,
    /// The next paragraph.
    /// If [`None`], the game meets the end.
    pub next: Option<String>,
}

/// The ayaka-game config.
/// It should be deserialized from a YAML file.
#[derive(Debug, Default, Deserialize)]
pub struct Game {
    /// The title of the game.
    pub title: String,
    /// The author of the game.
    #[serde(default)]
    pub author: String,
    /// The paragraphs, indexed by locale.
    pub paras: HashMap<Locale, Vec<Paragraph>>,
    /// The plugin config.
    #[serde(default)]
    pub plugins: PluginConfig,
    /// The global game properties.
    #[serde(default)]
    pub props: HashMap<String, String>,
    /// The resources, indexed by locale.
    #[serde(default)]
    pub res: HashMap<Locale, VarMap>,
    /// The base language.
    /// If the runtime fails to choose a best match,
    /// it fallbacks to this one.
    pub base_lang: Locale,
}

/// The plugin config.
#[derive(Debug, Default, Deserialize)]
pub struct PluginConfig {
    /// The directory of the plugins.
    pub dir: PathBuf,
    /// The names of the plugins, without extension.
    #[serde(default)]
    pub modules: Vec<String>,
}

impl Game {
    fn choose_from_keys<'a, V>(&'a self, loc: &Locale, map: &'a HashMap<Locale, V>) -> &'a Locale {
        loc.choose_from(map.keys()).unwrap_or(&self.base_lang)
    }

    fn find_para(&self, loc: &Locale, tag: &str) -> Option<&Paragraph> {
        if let Some(paras) = self.paras.get(loc) {
            for p in paras {
                if p.tag == tag {
                    return Some(p);
                }
            }
        }
        None
    }

    /// Find a paragraph by tag, with specified locale.
    pub fn find_para_fallback(&self, loc: &Locale, tag: &str) -> Fallback<&Paragraph> {
        let key = self.choose_from_keys(loc, &self.paras);
        let base_key = self.choose_from_keys(&self.base_lang, &self.paras);
        Fallback::new(
            if key == base_key {
                None
            } else {
                self.find_para(key, tag)
            },
            self.find_para(base_key, tag),
        )
    }

    fn find_res(&self, loc: &Locale) -> Option<&VarMap> {
        self.res.get(loc)
    }

    /// Find the resource map with specified locale.
    pub fn find_res_fallback(&self, loc: &Locale) -> Fallback<&VarMap> {
        let key = self.choose_from_keys(loc, &self.res);
        let base_key = self.choose_from_keys(&self.base_lang, &self.res);
        Fallback::new(
            if key == base_key {
                None
            } else {
                self.find_res(key)
            },
            self.find_res(base_key),
        )
    }
}
