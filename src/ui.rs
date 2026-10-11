use crate::{
    backend::{self, App, Event, Kind, Package, Source},
    design::{self as d, Glyph},
    icons::Icons,
};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, RichText, Stroke, Vec2};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};
#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Install,
    Library,
    Activity,
    Tools,
}
#[derive(Clone)]
enum Confirm {
    Prepare(Package),
    Install(Package, Option<PathBuf>, bool),
    Remove(App),
    Repository(String),
    RefreshDatabase,
}
#[derive(Clone, Debug)]
struct Repo {
    id: String,
    name: String,
    version: String,
    description: String,
    installed: bool,
}
fn repo_results(text: &str) -> Vec<Repo> {
    let mut rows: Vec<Repo> = vec![];
    for line in text.lines() {
        if line.starts_with(char::is_whitespace) {
            if let Some(last) = rows.last_mut() {
                if !last.description.is_empty() {
                    last.description.push(' ')
                }
                last.description.push_str(line.trim());
            }
        } else {
            let mut fields = line.split_whitespace();
            let Some(id) = fields.next() else { continue };
            let Some((_, name)) = id.split_once('/') else {
                continue;
            };
            rows.push(Repo {
                id: id.into(),
                name: name.into(),
                version: fields.next().unwrap_or("").into(),
                description: String::new(),
                installed: line.contains("[installed"),
            });
        }
    }
    rows
}
pub struct Dropdock {
    tab: Tab,
    apps: Vec<App>,
    packages: Vec<Package>,
    selected: usize,
    search: String,
    all_packages: bool,
    filter: usize,
    logs: Vec<String>,
    status: String,
    busy: bool,
    loading: bool,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    confirm: Option<(Confirm, String)>,
    executable: Option<PathBuf>,
    terminal: bool,
    repo_query: String,
    repos: Vec<Repo>,
    repo_searched: bool,
    error: Option<String>,
    notice: Option<(String, Instant)>,
    selected_app: Option<App>,
    icons: Icons,
    qa: Option<(PathBuf, Instant, bool)>,
    qa_scene: String,
    qa_applied: bool,
    focus_search: bool,
}
impl Dropdock {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        d::setup(&cc.egui_ctx);
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            tab: Tab::Install,
            apps: vec![],
            packages: vec![],
            selected: 0,
            search: std::env::var("DROPDOCK_SEARCH").unwrap_or_default(),
            all_packages: false,
            filter: 0,
            logs: std::fs::read_to_string(backend::data().join("activity.log"))
                .unwrap_or_default()
                .lines()
                .rev()
                .take(5000)
                .map(str::to_string)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect(),
            status: "Ready to install".into(),
            busy: false,
            loading: false,
            tx,
            rx,
            confirm: None,
            executable: None,
            terminal: false,
            repo_query: String::new(),
            repos: vec![],
            repo_searched: false,
            error: None,
            notice: None,
            selected_app: None,
            icons: Icons::new(),
            qa: std::env::var_os("DROPDOCK_SCREENSHOT").map(|p| (p.into(), Instant::now(), false)),
            qa_scene: std::env::var("DROPDOCK_PREVIEW").unwrap_or_default(),
            qa_applied: false,
            focus_search: false,
        };
        if app.qa.is_some() {
            app.tab = match app.qa_scene.as_str() {
                "library" | "details" | "uninstall" => Tab::Library,
                "tools" | "tools-results" => Tab::Tools,
                "activity" => Tab::Activity,
                _ => Tab::Install,
            };
        }
        app.refresh();
        let paths = std::env::args_os()
            .skip(1)
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .collect::<Vec<_>>();
        if !paths.is_empty() {
            app.inspect(paths)
        }
        app
    }
    fn refresh(&mut self) {
        if self.loading {
            return;
        }
        self.loading = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || match backend::inventory() {
            Ok(v) => {
                tx.send(Event::Inventory(v)).ok();
            }
            Err(e) => {
                tx.send(Event::InventoryError(e)).ok();
            }
        });
    }
    fn inspect(&mut self, paths: Vec<PathBuf>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        self.notice = None;
        self.packages.clear();
        self.selected = 0;
        self.executable = None;
        self.status = "Inspecting your files".into();
        self.tab = Tab::Install;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let mut packages = vec![];
            let mut errors = vec![];
            for path in paths {
                match backend::inspect(path.clone()) {
                    Ok(p) => packages.push(p),
                    Err(e) => errors.push(format!(
                        "{}: {e}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    )),
                }
            }
            tx.send(Event::Inspected(packages, errors)).ok();
        });
    }
    fn work<F>(&mut self, label: &str, job: F)
    where
        F: FnOnce(&Sender<Event>) -> Result<(), String> + Send + 'static,
    {
        self.busy = true;
        self.error = None;
        self.notice = None;
        self.status = label.into();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = backend::operation_lock().and_then(|_guard| job(&tx));
            if let Err(e) = result {
                tx.send(Event::Error(e)).ok();
            }
        });
    }
    fn log(&mut self, line: String) {
        use std::io::Write;
        std::fs::create_dir_all(backend::data()).ok();
        let path = backend::data().join("activity.log");
        if std::fs::metadata(&path).is_ok_and(|m| m.len() > 2_000_000) {
            std::fs::rename(&path, backend::data().join("activity.previous.log")).ok();
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            writeln!(f, "{line}").ok();
        }
        self.logs.push(line);
        if self.logs.len() > 5000 {
            self.logs.drain(..1000);
        }
    }
    fn events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Log(s) => self.log(s),
                Event::Inventory(v) => {
                    self.loading = false;
                    self.apps = v;
                }
                Event::InventoryError(e) => {
                    self.loading = false;
                    self.error = Some(format!("Couldn't refresh the library. {e}"));
                }
                Event::Inspected(v, errors) => {
                    self.busy = false;
                    self.packages = v;
                    self.selected = 0;
                    self.executable = None;
                    self.terminal = false;
                    if errors.is_empty() {
                        self.status = "Ready for review".into();
                    } else {
                        for message in &errors {
                            self.log(format!("ERROR: {message}"));
                        }
                        self.error = Some(if errors.len() == 1 {
                            errors[0].clone()
                        } else {
                            format!(
                                "{} files couldn't be opened. See Activity for details.",
                                errors.len()
                            )
                        });
                        self.status = "Some files need attention".into();
                    }
                }
                Event::Prepared(v) => {
                    self.busy = false;
                    if !v.is_empty() {
                        self.packages = v;
                        self.selected = 0;
                        self.executable = None;
                        self.terminal = false;
                        self.status = "Ready for review".into();
                    }
                }
                Event::Success(s) => {
                    self.busy = false;
                    self.error = None;
                    self.status = "Ready".into();
                    self.notice = Some((s.clone(), Instant::now()));
                    self.log(s);
                    self.refresh();
                }
                Event::Error(e) => {
                    self.busy = false;
                    self.status = "Operation needs attention".into();
                    self.error = Some(e.clone());
                    self.log(format!("ERROR: {e}"));
                }
                Event::InstallReview(p, details) => {
                    self.busy = false;
                    self.confirm = Some((
                        Confirm::Install(p, self.executable.clone(), self.terminal),
                        details,
                    ));
                }
                Event::RemoveReview(a, details) => {
                    self.busy = false;
                    self.confirm = Some((Confirm::Remove(a), details));
                }
                Event::SearchResults(s) => {
                    self.busy = false;
                    self.repos = repo_results(&s);
                    self.repo_searched = true;
                    self.status = "Search complete".into();
                }
            }
        }
    }
    fn choose_files(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .set_title("Choose an application package")
            .pick_files()
        {
            self.inspect(paths)
        }
    }
    fn choose_folder(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Choose a portable app or project folder")
            .pick_folder()
        {
            self.inspect(vec![path])
        }
    }
    fn request_remove(&mut self, a: App) {
        self.selected_app = None;
        self.work("Preparing removal details", move |tx| {
            let detail = backend::removal_preview(&a)?;
            tx.send(Event::RemoveReview(a, detail)).ok();
            Ok(())
        });
    }
    fn package_action(&mut self, p: Package) {
        match p.kind{
            Kind::Deb|Kind::Cargo|Kind::Pkgbuild=>self.confirm=Some((Confirm::Prepare(p.clone()),p.detail.clone())),
            Kind::Arch=>self.work("Checking dependencies",move|tx|{let preview=backend::arch_preview(&p)?;tx.send(Event::InstallReview(p,format!("Packages in this transaction:\n\n{preview}\nAdministrator authentication follows."))).ok();Ok(())}),
            Kind::Rpm=>{self.tab=Tab::Tools;self.repo_query=p.name.trim_end_matches(".rpm").split('-').next().unwrap_or("").into();},
            Kind::Unknown=>{},_=>self.confirm=Some((Confirm::Install(p.clone(),self.executable.clone(),self.terminal),p.detail.clone())),
        }
    }
    fn icon_for(&mut self, ui: &mut egui::Ui, app: &App, size: f32) {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
        self.paint_app_icon(ui, app, r);
    }
    fn paint_app_icon(&mut self, ui: &egui::Ui, app: &App, r: Rect) {
        if let Some(texture) = self.icons.get(ui.ctx(), app) {
            ui.painter().image(
                texture,
                r,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            ui.painter().rect_filled(r, 12.0, d::RAISED);
            d::glyph(
                ui.painter(),
                r.shrink(r.width() * 0.23),
                Glyph::Package,
                d::MUTED,
            );
        }
    }
    fn nav(&mut self, ui: &mut egui::Ui) {
        ui.add_space(15.0);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(31.0), egui::Sense::hover());
            ui.painter().rect_filled(r, 8.0, d::RAISED);
            d::glyph(ui.painter(), r.shrink(6.0), Glyph::Import, d::BLUE);
            ui.label(RichText::new("Dropdock").font(d::medium(23.0)));
        });
        ui.add_space(31.0);
        ui.label(
            RichText::new("WORKSPACE")
                .font(d::medium(10.5))
                .color(d::FAINT),
        );
        ui.add_space(7.0);
        for (tab, label, glyph) in [
            (Tab::Install, "Install an app", Glyph::Import),
            (Tab::Library, "My applications", Glyph::Grid),
            (Tab::Activity, "Activity", Glyph::Activity),
            (Tab::Tools, "Tools", Glyph::Settings),
        ] {
            let (r, response) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), egui::Sense::click());
            let selected = self.tab == tab;
            let t = ui
                .ctx()
                .animate_bool(response.id.with("hover"), response.hovered());
            if selected || t > 0.0 {
                ui.painter().rect_filled(
                    r,
                    9.0,
                    if selected {
                        Color32::from_rgb(37, 46, 62)
                    } else {
                        Color32::from_rgba_unmultiplied(70, 78, 92, (t * 30.0) as u8)
                    },
                );
            }
            let color = if selected {
                d::BLUE
            } else if response.hovered() {
                d::TEXT
            } else {
                d::MUTED
            };
            d::glyph(
                ui.painter(),
                Rect::from_center_size(Pos2::new(r.left() + 22.0, r.center().y), Vec2::splat(19.0)),
                glyph,
                color,
            );
            ui.painter().text(
                Pos2::new(r.left() + 43.0, r.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                d::medium(14.0),
                color,
            );
            if response.clicked() {
                self.tab = tab;
            }
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.add_space(5.0);
            ui.label(
                RichText::new("Ctrl + O  to add an app")
                    .font(d::regular(11.0))
                    .color(d::FAINT),
            );
            ui.add_space(14.0);
            d::surface()
                .inner_margin(13)
                .corner_radius(10)
                .show(ui, |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), 54.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let (r, _) =
                                    ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
                                ui.painter().circle_filled(r.center(), 3.0, d::GREEN);
                                ui.label(RichText::new("Your desktop").font(d::medium(12.0)));
                            });
                            ui.label(
                                RichText::new(if self.loading {
                                    "Refreshing library…".into()
                                } else {
                                    format!(
                                        "{} application entries",
                                        self.apps.iter().filter(|a| a.is_app).count()
                                    )
                                })
                                .font(d::regular(11.0))
                                .color(d::MUTED),
                            );
                        },
                    );
                });
        });
    }
    fn install_ui(&mut self, ui: &mut egui::Ui) {
        d::header(
            ui,
            "Install an app",
            "Bring a package or project. We’ll help with the rest.",
        );
        if !self.packages.is_empty() {
            self.package_review(ui);
            return;
        }
        let dragging = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        let tall = ui.available_height() > 510.0;
        d::surface()
            .fill(Color32::from_rgb(24, 28, 35))
            .stroke(Stroke::new(
                1.0_f32,
                if dragging {
                    d::BLUE
                } else {
                    Color32::from_rgb(59, 70, 88)
                },
            ))
            .inner_margin(28)
            .corner_radius(21)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    ui.add_space(if tall { 20.0 } else { 6.0 });
                    let (r, _) =
                        ui.allocate_exact_size(Vec2::new(250.0, 120.0), egui::Sense::hover());
                    d::hero(ui.painter(), r, dragging);
                    ui.add_space(10.0);
                    ui.label(RichText::new("Drop your app here").font(d::medium(24.0)));
                    ui.label(
                        RichText::new("A package file, portable app, or project folder.")
                            .font(d::regular(14.0))
                            .color(d::MUTED),
                    );
                    ui.add_space(13.0);
                    ui.horizontal(|ui| {
                        let total = 289.0;
                        ui.add_space(((ui.available_width() - total) / 2.0).max(0.0));
                        if ui
                            .add_enabled(
                                !self.busy,
                                egui::Button::new(
                                    RichText::new("Choose a file")
                                        .font(d::medium(14.0))
                                        .color(Color32::from_rgb(21, 33, 52)),
                                )
                                .fill(d::BLUE)
                                .min_size(Vec2::new(137.0, 40.0))
                                .corner_radius(9),
                            )
                            .clicked()
                        {
                            self.choose_files();
                        }
                        if ui
                            .add_enabled(
                                !self.busy,
                                egui::Button::new("Choose a folder")
                                    .min_size(Vec2::new(142.0, 40.0)),
                            )
                            .clicked()
                        {
                            self.choose_folder();
                        }
                    });
                    ui.add_space(16.0);
                    ui.label(
                        RichText::new(".deb   ·   Arch   ·   AppImage   ·   Flatpak   ·   Folders")
                            .font(d::regular(12.0))
                            .color(d::FAINT),
                    );
                    ui.add_space(if tall { 18.0 } else { 5.0 });
                });
            });
        ui.add_space(24.0);
        ui.columns(3, |cols| {
            for (col, (num, title, body)) in cols.iter_mut().zip([
                ("01", "Add your app", "Drop a file or choose a folder."),
                (
                    "02",
                    "Review the details",
                    "See what will change before installing.",
                ),
                (
                    "03",
                    "Make yourself at home",
                    "Launch it from your application menu.",
                ),
            ]) {
                col.horizontal(|ui| {
                    ui.label(RichText::new(num).font(d::regular(12.0)).color(d::FAINT));
                    ui.label(RichText::new(title).font(d::medium(13.5)));
                });
                col.label(RichText::new(body).font(d::regular(12.5)).color(d::MUTED));
            }
        });
        ui.add_space(24.0);
        ui.horizontal(|ui| {
            d::icon(ui, Glyph::Shield, 16.0, d::FAINT);
            ui.label(
                RichText::new("Nothing installs until you confirm.")
                    .font(d::regular(12.0))
                    .color(d::FAINT),
            );
        });
    }
    fn package_review(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.busy,
                    egui::Button::new("Choose another app").frame(false),
                )
                .clicked()
            {
                self.packages.clear();
                self.executable = None;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                d::pill(ui, "READY TO REVIEW", d::GREEN);
            });
        });
        ui.add_space(8.0);
        if self.packages.is_empty() {
            return;
        }
        if self.packages.len() > 1 {
            egui::ComboBox::from_id_salt("package-choice")
                .width(ui.available_width() - 20.0)
                .selected_text(&self.packages[self.selected].name)
                .show_ui(ui, |ui| {
                    for (i, p) in self.packages.iter().enumerate() {
                        if ui
                            .selectable_value(&mut self.selected, i, &p.name)
                            .changed()
                        {
                            self.executable = None;
                        }
                    }
                });
        }
        let p = self.packages[self.selected].clone();
        let native = p.kind == Kind::Arch;
        d::surface().inner_margin(25).show(ui,|ui|{
            ui.set_min_width(ui.available_width());ui.horizontal(|ui|{let(r,_)=ui.allocate_exact_size(Vec2::splat(60.0),egui::Sense::hover());ui.painter().rect_filled(r,15.0,d::RAISED);d::glyph(ui.painter(),r.shrink(14.0),if p.path.is_dir(){Glyph::Folder}else{Glyph::Package},d::BLUE);
                ui.vertical(|ui|{ui.label(RichText::new(p.label()).font(d::medium(10.5)).color(d::BLUE));ui.add(egui::Label::new(RichText::new(&p.name).font(d::medium(21.0))).truncate());});});
            ui.add_space(15.0);ui.separator();ui.add_space(12.0);
            let summary=match p.kind{Kind::Deb=>"We’ll convert this Debian package, then show you the result before installing.",Kind::Arch=>"This package is ready for CachyOS. We’ll check its dependencies before installing.",Kind::AppImage=>"A portable application. We’ll keep a private copy and add it to your app menu.",Kind::FlatpakRef|Kind::FlatpakBundle=>"Install with Flatpak. Its permissions and final confirmation will open in a terminal.",Kind::Cargo=>"Build a private copy of this Rust project, then choose a binary to install.",Kind::Pkgbuild=>"Build this Arch recipe in a private workspace, then review the resulting package.",Kind::Portable=>"Keep the app’s files together and add a launcher to your application menu.",Kind::Rpm=>"This package was made for another distribution. Find a compatible Arch version instead.",Kind::Unknown=>"This format isn’t supported yet. Try an AppImage, Flatpak, Arch package, or portable folder."};
            ui.label(RichText::new(summary).font(d::regular(15.0)).color(d::TEXT));ui.add_space(13.0);
            egui::Grid::new("package-facts").spacing(Vec2::new(24.0,12.0)).show(ui,|ui|{
                ui.label(RichText::new("Destination").color(d::FAINT));ui.label(if matches!(p.kind,Kind::AppImage|Kind::Portable){"Your application library"}else if matches!(p.kind,Kind::FlatpakRef|Kind::FlatpakBundle){"Your Flatpak applications"}else if matches!(p.kind,Kind::Cargo|Kind::Pkgbuild|Kind::Deb){"Private preparation workspace"}else{"System applications"});ui.end_row();
                if let Ok(meta)=std::fs::metadata(&p.path){if meta.is_file(){ui.label(RichText::new("File size").color(d::FAINT));ui.label(format_size(meta.len()));ui.end_row();}}
                if native{for key in ["Version","Architecture"]{if let Some(value)=package_field(&p.detail,key){ui.label(RichText::new(key).color(d::FAINT));ui.label(value);ui.end_row();}}}
            });
            if p.kind==Kind::Portable{
                ui.add_space(16.0);if p.path.is_dir(){ui.label(RichText::new("Main executable").font(d::medium(13.0)));let choices=backend::executable_choices(&p.path);if choices.len()==1&&self.executable.is_none(){self.executable=choices.first().cloned();}
                    egui::ComboBox::from_id_salt("executable").width(ui.available_width()-20.0).selected_text(self.executable.as_ref().map(|x|x.strip_prefix(&p.path).unwrap_or(x).display().to_string()).unwrap_or("Choose the file that starts this app".into())).show_ui(ui,|ui|{for c in choices{let label=c.strip_prefix(&p.path).unwrap_or(&c).display().to_string();ui.selectable_value(&mut self.executable,Some(c),label);}});
                }ui.checkbox(&mut self.terminal,"Run in a terminal").on_hover_text("Enable for command-line applications.");
            }
            ui.add_space(16.0);ui.collapsing("Package details",|ui|{egui::ScrollArea::vertical().max_height(190.0).show(ui,|ui|{ui.label(RichText::new(&p.detail).font(d::regular(12.5)).color(d::MUTED));ui.add_space(10.0);ui.label(RichText::new(p.path.display().to_string()).font(d::regular(11.0)).color(d::FAINT));if !p.hash.is_empty(){ui.label(RichText::new(format!("SHA-256  {}",p.hash)).font(FontId::monospace(10.0)).color(d::FAINT));}});});
            ui.add_space(17.0);ui.separator();ui.add_space(12.0);
            ui.horizontal(|ui|{d::icon(ui,Glyph::Info,17.0,d::FAINT);ui.label(RichText::new(if matches!(p.kind,Kind::Cargo|Kind::Pkgbuild){"Build only projects you trust."}else{"Review before continuing."}).font(d::regular(12.0)).color(d::MUTED));ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                let enabled=!self.busy&&p.kind!=Kind::Unknown&&!(p.kind==Kind::Portable&&p.path.is_dir()&&self.executable.is_none());
                let label=match p.kind{Kind::Deb=>"Convert package",Kind::Cargo|Kind::Pkgbuild=>"Build project",Kind::Rpm=>"Find compatible app",_=>"Review installation"};
                if d::primary(ui,label,enabled).clicked(){self.package_action(p.clone());}
            });});
        });
    }
    fn library_ui(&mut self, ui: &mut egui::Ui) {
        d::header(
            ui,
            "My applications",
            "Everything installed, with a clear way to manage it.",
        );
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 240.0).max(180.0);
            egui::Frame::new()
                .fill(d::SURFACE)
                .stroke(Stroke::new(1.0_f32, d::LINE))
                .corner_radius(9)
                .inner_margin(egui::Margin::symmetric(11, 5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        d::icon(ui, Glyph::Search, 18.0, d::FAINT);
                        let r = ui.add_sized(
                            [width - 54.0, 28.0],
                            egui::TextEdit::singleline(&mut self.search)
                                .hint_text("Search your applications")
                                .font(d::regular(14.0))
                                .frame(false),
                        );
                        if self.focus_search {
                            r.request_focus();
                            self.focus_search = false;
                        }
                        if !self.search.is_empty()
                            && d::icon_button(ui, Glyph::Close, "Clear search").clicked()
                        {
                            self.search.clear();
                        }
                    });
                });
            let labels = [
                "All sources",
                "Arch packages",
                "Flatpak",
                "Dropdock",
                "External",
                "Snap",
                "Steam",
            ];
            egui::ComboBox::from_id_salt("source")
                .width(149.0)
                .selected_text(labels[self.filter])
                .show_ui(ui, |ui| {
                    for (i, label) in labels.iter().enumerate() {
                        ui.selectable_value(&mut self.filter, i, *label);
                    }
                });
            if d::icon_button(ui, Glyph::Refresh, "Refresh library").clicked() && !self.busy {
                self.refresh();
            }
        });
        ui.add_space(4.0);
        let query = self.search.to_lowercase();
        let rows: Vec<_> = self
            .apps
            .iter()
            .filter(|a| {
                (self.all_packages || a.is_app)
                    && (query.is_empty()
                        || format!("{} {}", a.name, a.id)
                            .to_lowercase()
                            .contains(&query))
                    && match self.filter {
                        1 => a.source == Source::Pacman,
                        2 => matches!(a.source, Source::FlatpakSystem | Source::FlatpakUser),
                        3 => a.source == Source::Local,
                        4 => a.source == Source::Shortcut,
                        5 => a.source == Source::Snap,
                        6 => a.source == Source::Steam,
                        _ => true,
                    }
            })
            .cloned()
            .collect();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} {}",
                    rows.len(),
                    if rows.len() == 1 { "result" } else { "results" }
                ))
                .font(d::regular(12.0))
                .color(d::FAINT),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.checkbox(
                    &mut self.all_packages,
                    RichText::new("Include system packages")
                        .font(d::regular(12.0))
                        .color(d::MUTED),
                );
            });
        });
        if self.loading && self.apps.is_empty() {
            empty_state(
                ui,
                Glyph::Refresh,
                "Loading your library",
                "Reading installed packages and application launchers.",
            );
            return;
        }
        if rows.is_empty() {
            empty_state(
                ui,
                Glyph::Search,
                "No matching applications",
                "Try a different name or another source.",
            );
            ui.vertical_centered(|ui| {
                if d::secondary(ui, "Clear filters").clicked() {
                    self.search.clear();
                    self.filter = 0;
                }
            });
            return;
        }
        if self.all_packages {
            ui.label(RichText::new("System components are included. Essential packages are protected from removal.").font(d::regular(11.5)).color(d::AMBER));
        }
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .id_salt("apps")
            .auto_shrink([false, false])
            .show_rows(ui, 69.0, rows.len(), |ui, range| {
                for i in range {
                    self.app_row(ui, &rows[i], i);
                }
            });
    }
    fn app_row(&mut self, ui: &mut egui::Ui, a: &App, index: usize) {
        let (r, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 69.0), egui::Sense::hover());
        ui.painter().rect_filled(
            r,
            11.0,
            if index.is_multiple_of(2) {
                d::SURFACE
            } else {
                Color32::from_rgb(21, 24, 30)
            },
        );
        let icon = Rect::from_min_size(r.min + Vec2::new(12.0, 12.0), Vec2::splat(45.0));
        self.paint_app_icon(ui, a, icon);
        let text_rect = Rect::from_min_max(
            r.min + Vec2::new(71.0, 11.0),
            Pos2::new((r.right() - 192.0).max(r.left() + 130.0), r.bottom() - 8.0),
        );
        let mut text = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
        text.spacing_mut().item_spacing.y = 3.0;
        if text
            .add(
                egui::Label::new(RichText::new(&a.name).font(d::medium(14.5)))
                    .truncate()
                    .sense(egui::Sense::click()),
            )
            .on_hover_text(&a.name)
            .clicked()
        {
            self.selected_app = Some(a.clone());
        }
        let subtitle = if a.version.is_empty() {
            a.source.label().to_string()
        } else {
            format!("{}  ·  {}", a.source.label(), a.version)
        };
        text.add(
            egui::Label::new(
                RichText::new(subtitle)
                    .font(d::regular(11.5))
                    .color(d::MUTED),
            )
            .truncate(),
        );
        let actions = Rect::from_min_max(
            Pos2::new(r.right() - 187.0, r.top() + 16.0),
            r.max - Vec2::new(12.0, 14.0),
        );
        let mut actions = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(actions)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        if a.source == Source::Shortcut {
            if actions
                .add(
                    egui::Button::new(
                        RichText::new("Manage")
                            .font(d::regular(12.5))
                            .color(d::MUTED),
                    )
                    .frame(false),
                )
                .on_hover_text("View the original installation details")
                .clicked()
            {
                self.selected_app = Some(a.clone());
            }
        } else if backend::protected(a) {
            actions.label(
                RichText::new("Protected")
                    .font(d::regular(11.5))
                    .color(d::FAINT),
            );
        } else if actions
            .add_enabled(
                !self.busy,
                egui::Button::new(RichText::new("Uninstall").font(d::regular(12.5)))
                    .min_size(Vec2::new(87.0, 32.0)),
            )
            .on_hover_text(if a.source == Source::Steam {
                "Review removal in Steam"
            } else {
                "Review before removing"
            })
            .clicked()
        {
            self.request_remove(a.clone());
        }
        if d::icon_button(&mut actions, Glyph::Info, "Application details").clicked() {
            self.selected_app = Some(a.clone());
        }
    }
    fn activity_ui(&mut self, ui: &mut egui::Ui) {
        d::header(
            ui,
            "Activity",
            "A clear record of what happened, and what’s happening now.",
        );
        if self.logs.is_empty() && !self.busy {
            empty_state(
                ui,
                Glyph::Activity,
                "All quiet here",
                "Your next installation or removal will appear here.",
            );
            return;
        }
        ui.horizontal(|ui| {
            if self.busy {
                ui.spinner();
                ui.label(RichText::new(&self.status).color(d::BLUE));
            } else {
                d::pill(ui, "RECENT OUTPUT", d::MUTED);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if d::secondary(ui, "Save log").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name("dropdock-log.txt")
                        .save_file()
                    {
                        if let Err(e) = std::fs::write(path, self.logs.join("\n")) {
                            self.error = Some(e.to_string());
                        }
                    }
                }
                if d::secondary(ui, "Copy").clicked() {
                    ui.ctx().copy_text(self.logs.join("\n"));
                    self.notice = Some(("Activity copied to clipboard".into(), Instant::now()));
                }
            });
        });
        ui.add_space(10.0);
        d::surface()
            .fill(Color32::from_rgb(18, 21, 26))
            .inner_margin(17)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .max_height(ui.available_height() - 10.0)
                    .show(ui, |ui| {
                        for line in &self.logs {
                            let color = if line.starts_with("ERROR:") {
                                d::RED
                            } else if line.starts_with('›') || line.starts_with('>') {
                                d::BLUE
                            } else {
                                d::MUTED
                            };
                            ui.label(
                                RichText::new(line)
                                    .font(FontId::monospace(12.0))
                                    .color(color),
                            );
                        }
                    });
            });
    }
    fn tools_ui(&mut self, ui: &mut egui::Ui) {
        d::header(
            ui,
            "Tools",
            "Find a compatible package and keep conversions running smoothly.",
        );
        d::surface().show(ui,|ui|{
            ui.set_min_width(ui.available_width());ui.horizontal(|ui|{d::icon(ui,Glyph::Search,22.0,d::BLUE);ui.label(RichText::new("Find a native package").font(d::medium(18.0)));});
            ui.label(RichText::new("Search CachyOS and Arch repositories by application name.").font(d::regular(13.0)).color(d::MUTED));ui.add_space(8.0);
            ui.horizontal(|ui|{let width=(ui.available_width()-100.0).max(130.0);let edit=ui.add_sized([width,36.0],egui::TextEdit::singleline(&mut self.repo_query).hint_text("For example, Firefox").font(d::regular(14.0)).margin(Vec2::new(10.0,8.0)));let enter=edit.lost_focus()&&ui.input(|i|i.key_pressed(egui::Key::Enter));if (d::primary(ui,"Search",!self.busy&&!self.repo_query.trim().is_empty()).clicked()||enter)&&!self.busy{let query=self.repo_query.clone();self.work("Searching repositories",move|tx|{tx.send(Event::SearchResults(backend::repository_search(&query)?)).ok();Ok(())});}});
            if self.repo_searched&&self.repos.is_empty(){ui.label(RichText::new("No packages found. Try a shorter application name.").color(d::MUTED));}
            let repos=self.repos.clone();egui::ScrollArea::vertical().id_salt("repository-results").max_height(250.0).show(ui,|ui|{for repo in repos.iter().take(100){ui.add_space(5.0);ui.separator();ui.horizontal(|ui|{ui.vertical(|ui|{ui.set_max_width((ui.available_width()-130.0).max(130.0));ui.add(egui::Label::new(RichText::new(&repo.name).font(d::medium(14.0))).truncate());ui.add(egui::Label::new(RichText::new(&repo.description).font(d::regular(12.0)).color(d::MUTED)).truncate());ui.label(RichText::new(format!("{}  ·  {}",repo.id,repo.version)).font(d::regular(10.5)).color(d::FAINT));});ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if repo.installed{d::pill(ui,"INSTALLED",d::GREEN);}else if ui.add_enabled(!self.busy,egui::Button::new("Install")).clicked(){self.confirm=Some((Confirm::Repository(repo.id.clone()),format!("Install {} from your enabled repositories. A terminal will show the full package transaction and ask for final confirmation.",repo.id)));}});});}});
        });
        ui.add_space(16.0);
        d::surface().show(ui,|ui|{ui.set_min_width(ui.available_width());ui.horizontal(|ui|{d::icon(ui,Glyph::Refresh,22.0,d::BLUE);ui.label(RichText::new("Conversion database").font(d::medium(18.0)));});ui.label(RichText::new("Refresh dependency mappings if a Debian conversion reports missing data.").font(d::regular(13.0)).color(d::MUTED));ui.add_space(8.0);ui.horizontal(|ui|{if ui.add_enabled(!self.busy,egui::Button::new("Refresh database")).clicked(){self.confirm=Some((Confirm::RefreshDatabase,"Download current Debian-to-Arch dependency mappings. This requires administrator authentication and can take several minutes.".into()));}ui.label(RichText::new("Uses debtap").font(d::regular(11.5)).color(d::FAINT));});});
        ui.add_space(20.0);
        ui.collapsing(RichText::new("Supported formats & limitations").font(d::medium(13.0)),|ui|{ui.label(RichText::new("Install: Arch packages, AppImages, Flatpak references and bundles, portable folders. Prepare: Debian packages, Cargo projects and PKGBUILD recipes. RPM files use native-package search. Other source repositories need a compatible binary or supported build recipe. Local apps use manual updates.").font(d::regular(13.0)).color(d::MUTED));});
        if ui
            .add(
                egui::Button::new(
                    RichText::new("Read the usage guide")
                        .font(d::regular(12.0))
                        .color(d::BLUE),
                )
                .frame(false),
            )
            .clicked()
        {
            if let Ok(exe) = std::env::current_exe() {
                let mut guide = exe
                    .parent()
                    .unwrap_or(std::path::Path::new("."))
                    .join("README.md");
                if !guide.exists() {
                    guide = backend::home().join("Schreibtisch/Dropdock/README.md");
                }
                std::process::Command::new("/usr/bin/xdg-open")
                    .arg(guide)
                    .spawn()
                    .ok();
            }
        }
    }
    fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some((action, detail)) = self.confirm.clone() else {
            return;
        };
        let mut accept = false;
        let mut cancel = false;
        let destructive = matches!(action, Confirm::Remove(_));
        let response=egui::Modal::new(egui::Id::new("confirm-action")).backdrop_color(Color32::from_black_alpha(175)).frame(d::surface().inner_margin(28).corner_radius(20)).show(ctx,|ui|{
            ui.set_width((ctx.screen_rect().width()-100.0).clamp(350.0,540.0));
            ui.horizontal(|ui|{let(r,_)=ui.allocate_exact_size(Vec2::splat(42.0),egui::Sense::hover());ui.painter().rect_filled(r,11.0,if destructive{Color32::from_rgb(56,34,39)}else{Color32::from_rgb(34,45,63)});d::glyph(ui.painter(),r.shrink(10.0),if destructive{Glyph::Trash}else{Glyph::Package},if destructive{d::RED}else{d::BLUE});ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if d::icon_button(ui,Glyph::Close,"Cancel").clicked(){cancel=true}});});
            ui.add_space(14.0);
            let title=match action{Confirm::Remove(_)=>"Uninstall this application?",Confirm::Prepare(_)=>"Prepare this application?",Confirm::RefreshDatabase=>"Refresh conversion data?",_=>"Install this application?"};
            ui.label(RichText::new(title).font(d::display(27.0)));ui.add_space(2.0);
            let name=match &action{Confirm::Prepare(p)|Confirm::Install(p,_,_)=>&p.name,Confirm::Remove(a)=>&a.name,Confirm::Repository(n)=>n,Confirm::RefreshDatabase=>"Debian-to-Arch mappings"};
            ui.add(egui::Label::new(RichText::new(name).font(d::medium(15.0)).color(d::TEXT)).wrap());ui.add_space(10.0);
            egui::ScrollArea::vertical().max_height((ctx.screen_rect().height()-340.0).clamp(100.0,290.0)).show(ui,|ui|{
                if let Confirm::Remove(a)=&action{
                    let summary=match a.source{Source::Pacman=>"The owning package will be removed, including any other applications it contains. Personal files stay in place.",Source::FlatpakUser|Source::FlatpakSystem=>"The application will be removed. Your saved application data and shared runtimes will remain.",Source::Snap=>"Snap will remove the application using its normal snapshot settings.",Source::Steam=>"Steam will open its own uninstall dialog. Finish the removal there, then refresh your library.",Source::Local=>"Dropdock’s installed copy and launcher will be removed. Your original download and personal configuration will remain.",Source::Shortcut=>"This application is managed outside Dropdock."};
                    ui.label(RichText::new(summary).font(d::regular(14.0)).color(d::MUTED));ui.add_space(12.0);
                    ui.horizontal(|ui|{d::pill(ui,a.source.label(),d::MUTED);});
                    if a.source==Source::Pacman{ui.collapsing("Package and dependency details",|ui|{ui.label(RichText::new(&detail).font(FontId::monospace(11.0)).color(d::MUTED));});}
                }else{ui.label(RichText::new(&detail).font(d::regular(14.0)).color(d::MUTED));}
            });
            ui.add_space(20.0);ui.separator();ui.add_space(10.0);ui.horizontal(|ui|{
                if d::secondary(ui,"Cancel").clicked(){cancel=true}ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                    let label=match &action{Confirm::Remove(a)if a.source==Source::Steam=>"Continue in Steam",Confirm::Remove(_)=>"Uninstall",Confirm::Prepare(p)if p.kind==Kind::Deb=>"Convert package",Confirm::Prepare(_)=>"Build project",Confirm::RefreshDatabase=>"Refresh database",_=>"Install"};
                    if destructive{accept=ui.add(egui::Button::new(RichText::new(label).font(d::medium(14.0)).color(Color32::from_rgb(44,20,25))).fill(d::RED).corner_radius(9).min_size(Vec2::new(110.0,38.0))).clicked();}else{accept=d::primary(ui,label,true).clicked();}
                });
            });
        });
        if cancel || response.should_close() {
            self.confirm = None;
        }
        if accept {
            self.confirm = None;
            match action{
            Confirm::Prepare(p)=>self.work("Preparing your application",move|tx|{let result=backend::prepare(p,tx)?;tx.send(Event::Prepared(result)).ok();Ok(())}),
            Confirm::Install(p,exe,terminal)=>self.work("Installing your application",move|tx|{let name=p.name.clone();backend::install(p,exe,terminal,tx)?;tx.send(Event::Success(format!("Installed {name}"))).ok();Ok(())}),
            Confirm::Remove(a)=>self.work("Removing application",move|tx|{let name=a.name.clone();let steam=a.source==Source::Steam;backend::uninstall(a,tx)?;tx.send(Event::Success(if steam{format!("Finish removing {name} in Steam, then refresh the library.")}else{format!("Uninstalled {name}")})).ok();Ok(())}),
            Confirm::Repository(name)=>self.work("Waiting for the package manager",move|tx|{backend::install_repository(&name,tx)?;tx.send(Event::Success("Package-manager session finished. Refresh the library to check its result.".into())).ok();Ok(())}),
            Confirm::RefreshDatabase=>self.work("Refreshing conversion data",move|tx|{backend::refresh_debtap(tx)?;tx.send(Event::Success("Conversion data updated".into())).ok();Ok(())}),
        }
        }
    }
    fn app_dialog(&mut self, ctx: &egui::Context) {
        let Some(a) = self.selected_app.clone() else {
            return;
        };
        let mut close = false;
        let mut remove = false;
        let response=egui::Modal::new(egui::Id::new("application-details")).backdrop_color(Color32::from_black_alpha(175)).frame(d::surface().inner_margin(28).corner_radius(20)).show(ctx,|ui|{
            ui.set_width((ctx.screen_rect().width()-100.0).clamp(340.0,500.0));ui.horizontal(|ui|{self.icon_for(ui,&a,65.0);ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{close=d::icon_button(ui,Glyph::Close,"Close details").clicked();});});ui.add_space(17.0);
            ui.label(RichText::new(&a.name).font(d::display(29.0)));if !a.description.is_empty(){ui.label(RichText::new(&a.description).font(d::regular(13.5)).color(d::MUTED));}ui.add_space(18.0);
            egui::Grid::new("app-facts").spacing(Vec2::new(22.0,12.0)).show(ui,|ui|{
                ui.label(RichText::new("Managed by").color(d::FAINT));ui.label(a.source.label());ui.end_row();
                if !a.version.is_empty(){ui.label(RichText::new("Version").color(d::FAINT));ui.label(&a.version);ui.end_row();}
            });ui.add_space(12.0);
            if a.source==Source::Shortcut{d::surface().fill(Color32::from_rgb(33,32,30)).inner_margin(13).show(ui,|ui|{ui.label(RichText::new("Use the original installer").font(d::medium(13.0)).color(d::AMBER));ui.label(RichText::new("This launcher isn’t owned by a supported package manager. Removing its shortcut would not uninstall the application.").font(d::regular(12.5)).color(d::MUTED));});}
            ui.collapsing("Installation details",|ui|{ui.label(RichText::new(&a.id).font(FontId::monospace(11.0)).color(d::MUTED));if let Some(path)=&a.desktop{ui.label(RichText::new(path.display().to_string()).font(d::regular(11.0)).color(d::FAINT));}});
            ui.add_space(20.0);ui.separator();ui.add_space(10.0);ui.horizontal(|ui|{if d::secondary(ui,"Done").clicked(){close=true}ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if a.source!=Source::Shortcut&&!backend::protected(&a){remove=ui.add_enabled(!self.busy,egui::Button::new(RichText::new("Uninstall…").color(d::RED)).frame(false)).clicked();}else if backend::protected(&a){d::pill(ui,"SYSTEM COMPONENT",d::FAINT);}});});
        });
        if close || response.should_close() {
            self.selected_app = None
        }
        if remove {
            self.request_remove(a);
        }
    }
    fn banner(&mut self, ui: &mut egui::Ui) {
        if let Some(error) = self.error.clone() {
            egui::Frame::new()
                .fill(Color32::from_rgb(48, 29, 34))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(86, 48, 56)))
                .corner_radius(12)
                .inner_margin(14)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        d::icon(ui, Glyph::Info, 19.0, d::RED);
                        ui.label(
                            RichText::new("That didn’t complete")
                                .font(d::medium(14.0))
                                .color(d::RED),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if d::icon_button(ui, Glyph::Close, "Dismiss message").clicked() {
                                self.error = None;
                            }
                        });
                    });
                    ui.add(
                        egui::Label::new(
                            RichText::new(error.lines().next().unwrap_or(&error))
                                .font(d::regular(12.5))
                                .color(d::MUTED),
                        )
                        .wrap(),
                    );
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("View activity for details")
                                    .font(d::regular(12.0))
                                    .color(d::RED),
                            )
                            .frame(false),
                        )
                        .clicked()
                    {
                        self.tab = Tab::Activity;
                    }
                });
            ui.add_space(15.0);
        } else if let Some((text, start)) = self.notice.clone() {
            if start.elapsed() < Duration::from_secs(12) {
                egui::Frame::new()
                    .fill(Color32::from_rgb(24, 43, 36))
                    .corner_radius(11)
                    .inner_margin(13)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            d::icon(ui, Glyph::Check, 18.0, d::GREEN);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(text).font(d::regular(13.0)).color(d::GREEN),
                                )
                                .wrap(),
                            );
                        });
                    });
                ui.add_space(14.0);
            } else {
                self.notice = None
            }
        }
        if self.busy {
            egui::Frame::new()
                .fill(Color32::from_rgb(26, 36, 52))
                .corner_radius(11)
                .inner_margin(13)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            RichText::new(&self.status)
                                .font(d::medium(13.0))
                                .color(d::BLUE),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("View activity").font(d::regular(12.0)),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                self.tab = Tab::Activity;
                            }
                        });
                    });
                });
            ui.add_space(14.0);
        }
    }
    fn preview(&mut self) {
        if self.qa.is_none() || self.qa_applied {
            return;
        }
        match self.qa_scene.as_str() {
            "details" | "uninstall" => {
                if let Some(app) = self.apps.iter().find(|a| a.name == "Alacritty").cloned() {
                    if self.qa_scene == "details" {
                        self.selected_app = Some(app)
                    } else {
                        self.confirm = Some((
                            Confirm::Remove(app.clone()),
                            backend::removal_preview(&app).unwrap_or_default(),
                        ));
                    }
                    self.qa_applied = true;
                }
            }
            "review" => {
                if let Some(p) = self.packages.first().cloned() {
                    self.confirm = Some((Confirm::Prepare(p.clone()), p.detail.clone()));
                    self.qa_applied = true;
                }
            }
            "tools-results" => {
                self.repo_query = "firefox".into();
                self.work("Searching repositories", |tx| {
                    tx.send(Event::SearchResults(backend::repository_search("firefox")?))
                        .ok();
                    Ok(())
                });
                self.qa_applied = true;
            }
            "success" => {
                self.notice = Some((
                    "Application installed. Find it in your app menu.".into(),
                    Instant::now(),
                ));
                self.qa_applied = true;
            }
            "error" => {
                self.error=Some("The package could not be prepared. Check the activity log for the missing dependency.".into());
                self.qa_applied = true;
            }
            "busy" => {
                self.busy = true;
                self.status = "Converting your Debian package".into();
                self.qa_applied = true;
            }
            _ => {
                self.qa_applied = true;
            }
        }
    }
}
impl eframe::App for Dropdock {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if self.busy && self.qa.is_none() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.status = "Finish the current operation before closing".into();
        }
        self.events();
        self.preview();
        if let Some((path, start, requested)) = &mut self.qa {
            if start.elapsed().as_secs_f32() > 3.5 && !*requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                *requested = true;
            }
            for event in ctx.input(|i| i.events.clone()) {
                if let egui::Event::Screenshot { image, .. } = event {
                    let pixels: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                    if let Err(e) = image::save_buffer(
                        &*path,
                        &pixels,
                        image.width() as u32,
                        image.height() as u32,
                        image::ColorType::Rgba8,
                    ) {
                        eprintln!("Screenshot failed: {e}")
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        ctx.request_repaint_after(if self.busy || self.loading || self.qa.is_some() {
            Duration::from_millis(100)
        } else {
            Duration::from_millis(500)
        });
        let modal = self.confirm.is_some() || self.selected_app.is_some();
        if !self.busy && !modal {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::O)) {
                self.choose_files();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::F)) {
                self.tab = Tab::Library;
                self.focus_search = true;
            }
        }
        let paths = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect::<Vec<_>>()
        });
        if !paths.is_empty() && !self.busy && !modal {
            self.inspect(paths);
        }
        egui::TopBottomPanel::bottom("status")
            .exact_height(36.0)
            .frame(
                egui::Frame::new()
                    .fill(d::SIDEBAR)
                    .inner_margin(egui::Margin::symmetric(20, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.busy {
                        ui.add(egui::Spinner::new().size(13.0).color(d::BLUE));
                    } else {
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
                        ui.painter().circle_filled(
                            r.center(),
                            2.5,
                            if self.error.is_some() {
                                d::AMBER
                            } else {
                                d::GREEN
                            },
                        );
                    }
                    ui.add(
                        egui::Label::new(
                            RichText::new(if self.busy {
                                self.status.as_str()
                            } else if self.error.is_some() {
                                "Needs attention"
                            } else {
                                "Ready"
                            })
                            .font(d::regular(11.0))
                            .color(d::MUTED),
                        )
                        .truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new("Dropdock 0.2")
                                .font(d::regular(11.0))
                                .color(d::FAINT),
                        );
                    });
                });
            });
        egui::SidePanel::left("sidebar")
            .exact_width(221.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(d::SIDEBAR).inner_margin(20))
            .show(ctx, |ui| self.nav(ui));
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(d::BG)
                    .inner_margin(egui::Margin::symmetric(32, 29)),
            )
            .show(ctx, |ui| {
                self.banner(ui);
                match self.tab {
                    Tab::Install => {
                        egui::ScrollArea::vertical()
                            .id_salt("install-page")
                            .show(ui, |ui| self.install_ui(ui));
                    }
                    Tab::Library => self.library_ui(ui),
                    Tab::Activity => self.activity_ui(ui),
                    Tab::Tools => {
                        egui::ScrollArea::vertical()
                            .id_salt("tools-page")
                            .show(ui, |ui| self.tools_ui(ui));
                    }
                }
            });
        self.app_dialog(ctx);
        self.confirm_dialog(ctx);
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) && !self.busy && !modal {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::new("file-drop"),
            ));
            let rect = ctx.screen_rect();
            painter.rect_filled(rect, 0.0, Color32::from_black_alpha(220));
            let r = Rect::from_center_size(rect.center() - Vec2::new(0.0, 32.0), Vec2::splat(64.0));
            d::glyph(&painter, r, Glyph::Import, d::BLUE);
            painter.text(
                rect.center() + Vec2::new(0.0, 36.0),
                egui::Align2::CENTER_CENTER,
                "Release to inspect your app",
                d::display(29.0),
                d::TEXT,
            );
            painter.text(
                rect.center() + Vec2::new(0.0, 72.0),
                egui::Align2::CENTER_CENTER,
                "Nothing installs until you confirm.",
                d::regular(14.0),
                d::MUTED,
            );
        }
    }
}
fn empty_state(ui: &mut egui::Ui, g: Glyph, title: &str, body: &str) {
    ui.add_space((ui.available_height() * 0.18).clamp(25.0, 95.0));
    ui.vertical_centered(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(62.0), egui::Sense::hover());
        ui.painter().rect_filled(r, 17.0, d::SURFACE);
        d::glyph(ui.painter(), r.shrink(18.0), g, d::FAINT);
        ui.add_space(13.0);
        ui.label(RichText::new(title).font(d::medium(20.0)));
        ui.label(RichText::new(body).font(d::regular(13.0)).color(d::MUTED));
        ui.add_space(15.0);
    });
}
fn format_size(size: u64) -> String {
    if size >= 1_000_000_000 {
        format!("{:.1} GB", size as f64 / 1e9)
    } else if size >= 1_000_000 {
        format!("{:.1} MB", size as f64 / 1e6)
    } else {
        format!("{:.0} KB", size as f64 / 1000.0)
    }
}
fn package_field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (k, v) = line.split_once(':')?;
        (k.trim() == key).then_some(v.trim())
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repository_rows() {
        let rows=repo_results("extra/firefox 147.0-1 [installed]\n    Fast browser\nextra/firefox-i18n-de 147.0-1\n    German language pack\n");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "extra/firefox");
        assert!(rows[0].installed);
        assert_eq!(rows[1].description, "German language pack");
    }
    #[test]
    fn package_fields() {
        assert_eq!(
            package_field("Name : demo\nVersion : 1.2-1\n", "Version"),
            Some("1.2-1")
        );
        assert_eq!(package_field("Name : demo", "Version"), None);
    }
}
