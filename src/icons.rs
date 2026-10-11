use crate::backend::{App, Source};
use eframe::egui::{self, ColorImage, TextureHandle};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{self, Receiver, Sender},
};
type Loaded = (String, Option<ColorImage>);
pub struct Icons {
    textures: HashMap<String, TextureHandle>,
    requested: HashSet<String>,
    tx: Sender<(String, App)>,
    rx: Receiver<Loaded>,
}
impl Icons {
    pub fn new() -> Self {
        let (tx, jobs) = mpsc::channel::<(String, App)>();
        let (done, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for (key, app) in jobs {
                let image = load(&app);
                if done.send((key, image)).is_err() {
                    break;
                }
            }
        });
        Self {
            textures: HashMap::new(),
            requested: HashSet::new(),
            tx,
            rx,
        }
    }
    fn key(a: &App) -> String {
        format!("{:?}:{}", a.source, a.id)
    }
    pub fn get(&mut self, ctx: &egui::Context, a: &App) -> Option<egui::TextureId> {
        while let Ok((key, image)) = self.rx.try_recv() {
            if let Some(image) = image {
                self.textures.insert(
                    key.clone(),
                    ctx.load_texture(key, image, egui::TextureOptions::LINEAR),
                );
            }
        }
        let key = Self::key(a);
        if let Some(t) = self.textures.get(&key) {
            return Some(t.id());
        }
        if self.requested.insert(key.clone()) {
            self.tx.send((key, a.clone())).ok();
        }
        None
    }
}
fn desktop_icon(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut active = false;
    for line in text.lines() {
        if line.starts_with('[') {
            active = line == "[Desktop Entry]";
        } else if active {
            if let Some(v) = line.strip_prefix("Icon=") {
                return Some(v.to_string());
            }
        }
    }
    None
}
fn load(app: &App) -> Option<ColorImage> {
    let mut names = vec![];
    if let Some(path) = &app.desktop {
        if let Some(icon) = desktop_icon(path) {
            names.push(icon)
        }
    }
    if app.source == Source::Steam {
        names.push(format!("steam_icon_{}", app.id));
    }
    names.push(app.id.split('/').next().unwrap_or(&app.id).into());
    for name in names {
        let path = if name.starts_with('/') {
            PathBuf::from(&name)
        } else {
            let out = Command::new("/usr/bin/kiconfinder6")
                .arg(&name)
                .output()
                .ok()?;
            if !out.status.success() {
                continue;
            }
            PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
        };
        if !path.is_file() {
            continue;
        }
        let bytes = if matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("svg" | "svgz")
        ) {
            let out = Command::new("/usr/bin/rsvg-convert")
                .args(["-w", "96", "-h", "96"])
                .arg(&path)
                .output()
                .ok()?;
            if !out.status.success() {
                continue;
            }
            out.stdout
        } else {
            std::fs::read(&path).ok()?
        };
        if let Ok(img) = image::load_from_memory(&bytes) {
            let img = img
                .resize(96, 96, image::imageops::FilterType::Lanczos3)
                .into_rgba8();
            return Some(ColorImage::from_rgba_unmultiplied(
                [img.width() as usize, img.height() as usize],
                &img,
            ));
        }
    }
    None
}
