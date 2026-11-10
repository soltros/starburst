mod config;
mod engine;
mod indexer;
use config::{Config, Provider};
use eframe::egui::{self, Color32, RichText};
use engine::{Command, Row, Update, Worker};
use std::{fs::OpenOptions, os::unix::fs::OpenOptionsExt};
use usenet_dl::Status;
const ACCENT: Color32 = Color32::from_rgb(245, 171, 105);
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Downloads,
    Search,
    History,
    Settings,
}
struct Starburst {
    config: Config,
    worker: Option<Worker>,
    page: Page,
    query: String,
    url: String,
    results: Vec<indexer::Release>,
    rows: Vec<Row>,
    notice: String,
    error: bool,
    searching: bool,
    ready: bool,
    filter: String,
    search_rx: Option<std::sync::mpsc::Receiver<(Vec<indexer::Release>, Vec<String>)>>,
}
fn field(ui: &mut egui::Ui, label: &str, value: &mut String, secret: bool) {
    ui.label(RichText::new(label).color(Color32::from_gray(165)));
    ui.add(
        egui::TextEdit::singleline(value)
            .password(secret)
            .desired_width(f32::INFINITY),
    );
    ui.add_space(7.0);
}
fn bytes(n: u64) -> String {
    if n >= 1 << 30 {
        format!("{:.2} GB", n as f64 / (1u64 << 30) as f64)
    } else {
        format!("{:.1} MB", n as f64 / (1u64 << 20) as f64)
    }
}
fn card(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(Color32::from_rgb(28, 30, 40))
        .rounding(12.0)
        .inner_margin(18.0)
        .show(ui, body);
    ui.add_space(10.0);
}
impl Starburst {
    fn new(cc: &eframe::CreationContext<'_>, config: Config, load_error: Option<String>) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = Color32::from_rgb(17, 19, 27);
        style.visuals.window_fill = Color32::from_rgb(28, 30, 40);
        style.visuals.selection.bg_fill = Color32::from_rgb(111, 72, 45);
        style.visuals.widgets.active.bg_fill = ACCENT;
        style.spacing.item_spacing = egui::vec2(12.0, 10.0);
        style.spacing.button_padding = egui::vec2(16.0, 10.0);
        style.visuals.widgets.inactive.rounding = 8.0.into();
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        cc.egui_ctx.set_style(style);
        let page = if config.providers.is_empty() {
            Page::Settings
        } else {
            Page::Downloads
        };
        let worker = if config.providers.is_empty() || load_error.is_some() {
            None
        } else {
            Some(Worker::start(config.clone(), cc.egui_ctx.clone()))
        };
        Self {
            config,
            worker,
            page,
            query: String::new(),
            url: String::new(),
            results: vec![],
            rows: vec![],
            notice: load_error
                .clone()
                .unwrap_or_else(|| "Your next discovery starts here.".into()),
            error: load_error.is_some(),
            searching: false,
            ready: false,
            filter: String::new(),
            search_rx: None,
        }
    }
    fn send(&self, c: Command) {
        if let Some(w) = &self.worker {
            w.send(c);
        }
    }
    fn import(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("NZB manifest", &["nzb"])
            .pick_file()
        {
            self.send(Command::AddFile(path));
        }
    }
    fn settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label("Connect your Usenet services. Credentials are stored locally with owner-only permissions.");
        ui.add_space(14.0);
        let mut remove = None;
        for (n, p) in self.config.providers.iter_mut().enumerate() {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(format!("Provider {}", n + 1));
                    if ui.small_button("Remove").clicked() {
                        remove = Some(n);
                    }
                });
                field(ui, "Display name", &mut p.name, false);
                field(ui, "Server hostname", &mut p.host, false);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut p.tls, "TLS encryption");
                    ui.label("Port");
                    ui.add(egui::DragValue::new(&mut p.port).range(1..=65535));
                    ui.label("Connections");
                    ui.add(egui::DragValue::new(&mut p.connections).range(1..=100));
                    ui.label("Priority");
                    ui.add(egui::DragValue::new(&mut p.level).range(0..=99));
                });
                field(ui, "Username", &mut p.username, false);
                field(ui, "Password", &mut p.password, true);
            });
        }
        if let Some(n) = remove {
            self.config.providers.remove(n);
        }
        if ui.button("+ Add provider").clicked() {
            self.config.providers.push(Provider::default());
        }
        ui.label(
            RichText::new(
                "Lower priority numbers are tried first; higher numbers are backup providers.",
            )
            .small()
            .weak(),
        );
        ui.add_space(18.0);
        let mut remove = None;
        for (n, i) in self.config.indexers.iter_mut().enumerate() {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(format!("Indexer {}", n + 1));
                    if ui.small_button("Remove").clicked() {
                        remove = Some(n);
                    }
                });
                let selected = indexer::PRESETS.iter().position(|p| p.url == i.url);
                let mut choice = selected;
                egui::ComboBox::from_id_salt(("indexer-service", n))
                    .selected_text(
                        selected
                            .map(|v| indexer::PRESETS[v].name)
                            .unwrap_or("Custom Newznab"),
                    )
                    .show_ui(ui, |ui| {
                        for (v, preset) in indexer::PRESETS.iter().enumerate() {
                            ui.selectable_value(&mut choice, Some(v), preset.name);
                        }
                        ui.selectable_value(&mut choice, None, "Custom Newznab");
                    });
                if choice != selected {
                    *i = choice
                        .map(|v| indexer::preset_indexer(&indexer::PRESETS[v]))
                        .unwrap_or_default();
                }
                if let Some(v) = choice {
                    ui.hyperlink_to(
                        "Open account website to find your API key",
                        indexer::PRESETS[v].site,
                    );
                } else {
                    field(ui, "Display name", &mut i.name, false);
                }
                if choice.is_none() {
                    field(ui, "Newznab API URL", &mut i.url, false);
                }
                field(ui, "API key", &mut i.api_key, true);
            });
        }
        if let Some(n) = remove {
            self.config.indexers.remove(n);
        }
        if ui.button("+ Add indexer").clicked() {
            self.config
                .indexers
                .push(indexer::preset_indexer(&indexer::PRESETS[0]));
        }
        ui.add_space(18.0);
        card(ui, |ui| {
            ui.heading("Files & folders");
            ui.horizontal(|ui| {
                ui.label(self.config.destination.display().to_string());
                if ui.button("Choose folder").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        self.config.destination = p;
                    }
                }
            });
            ui.checkbox(
                &mut self.config.unpack,
                "Automatically verify, repair and unpack downloads",
            );
            ui.label(RichText::new("Archives are kept after extraction. Settings changes take effect after restarting Starburst.").small().weak());
        });
        if ui
            .add(
                egui::Button::new(RichText::new("Save settings").color(Color32::BLACK))
                    .fill(ACCENT),
            )
            .clicked()
        {
            match self.config.save() {
                Ok(()) => {
                    self.error = false;
                    self.notice =
                        "Settings saved. Restart Starburst to apply connection or folder changes."
                            .into();
                    if self.worker.is_none() && !self.config.providers.is_empty() {
                        self.worker = Some(Worker::start(self.config.clone(), ctx.clone()));
                        self.notice = "Settings saved. Connecting to your providers…".into();
                    }
                }
                Err(e) => {
                    self.error = true;
                    self.notice = e.to_string();
                }
            }
        }
    }
    fn search(&mut self, ui: &mut egui::Ui) {
        ui.label("Select your indexer in Settings and paste its API key to search. A download provider is only needed to download files.");
        ui.add_space(18.0);
        let mut submit = false;
        ui.horizontal(|ui| {
            let r = ui.add_sized(
                [ui.available_width() - 110.0, 40.0],
                egui::TextEdit::singleline(&mut self.query).hint_text("What are you looking for?"),
            );
            submit = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui
                .add_enabled(!self.searching, egui::Button::new("Search"))
                .clicked()
            {
                submit = true;
            }
        });
        if submit && !self.query.trim().is_empty() && !self.config.indexers.is_empty() {
            self.searching = true;
            let indexers = self.config.indexers.clone();
            let query = self.query.clone();
            let ctx = ui.ctx().clone();
            let (tx, rx) = std::sync::mpsc::channel();
            self.search_rx = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(indexer::search_all(indexers, query));
                ctx.request_repaint();
            });
        }
        if self.config.indexers.is_empty() {
            ui.label("Add an indexer in Settings to start searching.");
        }
        if self.searching {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Searching your indexers…");
            });
        }
        ui.add_space(12.0);
        ui.label(format!("{} results", self.results.len()));
        let mut download = None;
        for r in &self.results {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&r.title).strong());
                        ui.label(
                            RichText::new(format!("{}  ·  {}", r.source, bytes(r.size)))
                                .small()
                                .weak(),
                        );
                    });
                    if ui.button("Download").clicked() {
                        download = Some((r.url.clone(), format!("{}.nzb", r.title)));
                    }
                });
            });
        }
        if let Some((url, name)) = download {
            if self.worker.is_none() {
                self.notice = "Add and save a Usenet download provider in Settings first.".into();
                self.error = true;
                return;
            }
            self.send(Command::AddUrl(url, name));
            self.notice = "Fetching NZB…".into();
            self.error = false;
        }
    }
    fn downloads(&mut self, ui: &mut egui::Ui, history: bool) {
        if !history {
            ui.horizontal(|ui| {
                if ui.button("+ Import NZB").clicked() {
                    self.import();
                }
                if ui.button("Pause all").clicked() {
                    self.send(Command::PauseAll);
                }
                if ui.button("Resume all").clicked() {
                    self.send(Command::ResumeAll);
                }
            });
            ui.horizontal(|ui| {
                ui.add_sized(
                    [ui.available_width() - 100.0, 34.0],
                    egui::TextEdit::singleline(&mut self.url)
                        .hint_text("Or paste an NZB download URL…"),
                );
                if ui.button("Add URL").clicked() && !self.url.trim().is_empty() {
                    self.send(Command::AddUrl(
                        self.url.trim().to_owned(),
                        "download.nzb".into(),
                    ));
                    self.url.clear();
                }
            });
        }
        field(ui, "Filter downloads", &mut self.filter, false);
        let speed: u64 = self
            .rows
            .iter()
            .filter(|r| r.status == Status::Downloading)
            .map(|r| r.speed)
            .sum();
        if !history {
            ui.label(
                RichText::new(format!("{} /s  ·  automatic repair & unpack", bytes(speed)))
                    .color(ACCENT),
            );
            ui.add_space(14.0);
        }
        let mut action = None;
        let mut count = 0;
        for r in &self.rows {
            let terminal = matches!(r.status, Status::Complete | Status::Failed);
            if terminal != history || !r.name.to_lowercase().contains(&self.filter.to_lowercase()) {
                continue;
            }
            count += 1;
            card(ui, |ui| {
                ui.label(RichText::new(&r.name).size(17.0).strong());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{:?}", r.status)).color(
                        if r.status == Status::Failed {
                            Color32::LIGHT_RED
                        } else {
                            ACCENT
                        },
                    ));
                    ui.label(format!("{}  ·  {}/s", bytes(r.size), bytes(r.speed)));
                });
                ui.add(
                    egui::ProgressBar::new(r.progress.clamp(0.0, 1.0))
                        .desired_width(f32::INFINITY)
                        .show_percentage(),
                );
                if let Some(e) = &r.error {
                    ui.colored_label(Color32::LIGHT_RED, e);
                }
                ui.horizontal(|ui| {
                    if !terminal {
                        if r.status == Status::Paused {
                            if ui.small_button("Resume").clicked() {
                                action = Some(Command::Resume(r.id));
                            }
                        } else if matches!(r.status, Status::Queued | Status::Downloading)
                            && ui.small_button("Pause").clicked()
                        {
                            action = Some(Command::Pause(r.id));
                        }
                        if ui.small_button("Cancel download").clicked() {
                            action = Some(Command::Cancel(r.id));
                        }
                    }
                });
            });
        }
        if let Some(c) = action {
            self.send(c);
        }
        if count == 0 {
            ui.add_space(50.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(if history {
                        "A fresh start."
                    } else {
                        "Make room for discovery."
                    })
                    .size(27.0)
                    .strong(),
                );
                ui.label(if history {
                    "Completed and failed downloads will appear here."
                } else {
                    "Search your indexers or drop an NZB file into this window."
                });
            });
        }
    }
}
impl eframe::App for Starburst {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(rx) = &self.search_rx {
            if let Ok((results, errors)) = rx.try_recv() {
                self.notice = if errors.is_empty() {
                    format!("Found {} results", results.len())
                } else {
                    errors.join("; ")
                };
                self.error = !errors.is_empty();
                self.results = results;
                self.searching = false;
                self.search_rx = None;
            }
        }
        if let Some(w) = &self.worker {
            while let Ok(u) = w.rx.try_recv() {
                match u {
                    Update::Ready => {
                        self.ready = true;
                        self.notice = "Ready for your next discovery.".into();
                        self.error = false;
                    }
                    Update::Message(m) => {
                        self.notice = m;
                        self.error = false;
                    }
                    Update::Error(m) => {
                        self.notice = m;
                        self.error = true;
                    }
                    Update::Queue(r) => self.rows = r,
                }
            }
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            if let Some(p) = f.path {
                if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("nzb")) {
                    self.send(Command::AddFile(p));
                }
            }
        }
        egui::SidePanel::left("navigation")
            .exact_width(210.0)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(22, 24, 34))
                    .inner_margin(22.0),
            )
            .show(ctx, |ui| {
                ui.add_space(16.0);
                ui.label(RichText::new("✦").size(44.0).color(ACCENT));
                ui.label(RichText::new("Starburst").size(26.0).strong());
                ui.label(RichText::new("A brighter way to Usenet").size(12.0).weak());
                ui.add_space(42.0);
                for (page, label) in [
                    (Page::Downloads, "↓  Downloads"),
                    (Page::Search, "⌕  Discover"),
                    (Page::History, "◷  History"),
                    (Page::Settings, "⚙  Settings"),
                ] {
                    if ui
                        .add_sized(
                            [166.0, 44.0],
                            egui::Button::new(label).selected(self.page == page),
                        )
                        .clicked()
                    {
                        self.page = page;
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.label(RichText::new("STARBURST  /  0.1.0").small().weak());
                    ui.label(if self.ready {
                        "● Engine ready"
                    } else {
                        "○ Configure your providers"
                    });
                });
            });
        egui::TopBottomPanel::bottom("notice").show(ctx, |ui| {
            ui.add_space(7.0);
            ui.colored_label(
                if self.error {
                    Color32::LIGHT_RED
                } else {
                    Color32::from_gray(170)
                },
                &self.notice,
            );
            ui.add_space(7.0);
        });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(17, 19, 27))
                    .inner_margin(32.0),
            )
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("YOUR PERSONAL USENET DESKTOP")
                        .size(11.0)
                        .color(ACCENT),
                );
                ui.add_space(8.0);
                ui.heading(
                    RichText::new(match self.page {
                        Page::Downloads => "Downloads",
                        Page::Search => "Discover",
                        Page::History => "History",
                        Page::Settings => "Make yourself at home.",
                    })
                    .size(34.0),
                );
                ui.add_space(22.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    match self.page {
                        Page::Settings => self.settings(ui, ctx),
                        Page::Search => self.search(ui),
                        Page::Downloads => self.downloads(ui, false),
                        Page::History => self.downloads(ui, true),
                    }
                });
            });
    }
}
fn main() -> anyhow::Result<()> {
    use fs2::FileExt;
    std::fs::create_dir_all(config::root())?;
    std::fs::set_permissions(
        config::root(),
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(config::root().join("instance.lock"))?;
    lock.try_lock_exclusive()
        .map_err(|_| anyhow::anyhow!("Starburst is already running"))?;
    let (config, error) = match Config::load() {
        Ok(c) => (c, None),
        Err(e) => (
            Config::default(),
            Some(format!(
                "Could not load settings: {e}. Existing settings have not been changed."
            )),
        ),
    };
    eframe::run_native(
        "Starburst",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1120.0, 780.0])
                .with_min_inner_size([820.0, 600.0]),
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(Starburst::new(cc, config, error)))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
