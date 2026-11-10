mod categories;
mod config;
mod diagnostics;
mod engine;
mod indexer;
mod manifest;
mod ui;
use config::{Config, Provider};
use eframe::egui::{self, Color32, RichText};
use engine::{Command, Row, Update, Worker};
use std::{fs::OpenOptions, os::unix::fs::OpenOptionsExt};
use ui::{Icon, ACCENT};
use usenet_dl::Status;
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
    category_root: u32,
    category_sub: u32,
    searched_category: u32,
    url: String,
    results: Vec<indexer::Release>,
    rows: Vec<Row>,
    notice: String,
    error: bool,
    searching: bool,
    ready: bool,
    filter: String,
    checks: diagnostics::Checks,
    search_errors: Vec<String>,
    search_finished: bool,
    search_rx: Option<std::sync::mpsc::Receiver<(Vec<indexer::Release>, Vec<String>)>>,
}
fn field(ui: &mut egui::Ui, label: &str, value: &mut String, secret: bool) {
    ui.label(RichText::new(label).size(12.0).color(ui::MUTED));
    ui.add(
        egui::TextEdit::singleline(value)
            .password(secret)
            .desired_width(f32::INFINITY)
            .margin(egui::vec2(12.0, 10.0)),
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
    let width = (ui.available_width() - 40.0).max(0.0);
    egui::Frame::none()
        .fill(ui::SURFACE)
        .stroke(egui::Stroke::new(1.0_f32, ui::BORDER))
        .rounding(12.0)
        .inner_margin(20.0)
        .show(ui, |ui| {
            ui.set_min_width(width);
            body(ui);
        });
    ui.add_space(14.0);
}
impl Starburst {
    fn new(cc: &eframe::CreationContext<'_>, config: Config, load_error: Option<String>) -> Self {
        ui::theme(&cc.egui_ctx);
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
            category_root: 0,
            category_sub: 0,
            searched_category: 0,
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
            checks: Default::default(),
            search_errors: vec![],
            search_finished: false,
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
    fn save_settings(&mut self, ctx: &egui::Context) {
        match self.config.save() {
            Ok(()) => {
                self.error = false;
                self.notice = "Saved. Indexers are ready; restart to apply changes to existing download connections.".into();
                if self.worker.is_none() && !self.config.providers.is_empty() {
                    self.worker = Some(Worker::start(self.config.clone(), ctx.clone()));
                }
            }
            Err(e) => {
                self.error = true;
                self.notice = e.to_string();
            }
        }
    }
    fn settings(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        if self.config.providers.is_empty() && self.config.indexers.is_empty() {
            let mut add_provider = false;
            let mut add_indexer = false;
            if ui.available_width() >= 750.0 {
                ui.columns(2, |columns| {
                    add_provider = ui::empty(
                        &mut columns[0],
                        Icon::Server,
                        "Your download provider",
                        "Connect the service that stores and delivers your files.",
                        "Add provider",
                    );
                    add_indexer = ui::empty(
                        &mut columns[1],
                        Icon::Search,
                        "Your search, connected",
                        "Choose NZBGeek or another indexer, then paste your API key.",
                        "Add indexer",
                    );
                });
            } else {
                add_provider = ui::empty(
                    ui,
                    Icon::Server,
                    "Your download provider",
                    "Connect the service that delivers your files.",
                    "Add provider",
                );
                add_indexer = ui::empty(
                    ui,
                    Icon::Search,
                    "Your search, connected",
                    "Choose an indexer and paste your API key.",
                    "Add indexer",
                );
            }
            if add_provider {
                self.config.providers.push(Provider::default());
            }
            if add_indexer {
                self.config
                    .indexers
                    .push(indexer::preset_indexer(&indexer::PRESETS[0]));
            }
        } else {
            ui::section(
                ui,
                Icon::Server,
                "Usenet providers",
                "Your download connections. Lower priority numbers are tried first.",
            );
            let mut remove = None;
            for (n, p) in self.config.providers.iter_mut().enumerate() {
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("Connection {}", n + 1))
                                .size(16.0)
                                .strong(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Remove").clicked() {
                                remove = Some(n);
                            }
                        });
                    });
                    ui.add_space(8.0);
                    ui.columns(2, |c| {
                        field(&mut c[0], "Display name", &mut p.name, false);
                        field(&mut c[1], "Server hostname", &mut p.host, false);
                    });
                    ui.columns(2, |c| {
                        field(&mut c[0], "Username", &mut p.username, false);
                        field(&mut c[1], "Password", &mut p.password, true);
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut p.tls, "TLS encryption");
                        ui.separator();
                        ui.label(RichText::new("Port").color(ui::MUTED));
                        ui.add(egui::DragValue::new(&mut p.port).range(1..=65535));
                        ui.label(RichText::new("Connections").color(ui::MUTED));
                        ui.add(egui::DragValue::new(&mut p.connections).range(1..=100));
                        ui.label(RichText::new("Priority").color(ui::MUTED));
                        ui.add(egui::DragValue::new(&mut p.level).range(0..=99));
                    });
                    diagnostics::control(
                        ui,
                        &mut self.checks,
                        n,
                        diagnostics::Service::Provider(p.clone()),
                    );
                });
            }
            if let Some(n) = remove {
                self.config.providers.remove(n);
            }
            if ui.button("Add provider").clicked() {
                self.config.providers.push(Provider::default());
            }
            ui.add_space(22.0);
            ui::section(
                ui,
                Icon::Search,
                "Search indexers",
                "Connect an API key to find releases across your services.",
            );
            let mut remove = None;
            for (n, i) in self.config.indexers.iter_mut().enumerate() {
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(if i.name.is_empty() {
                                "Custom indexer"
                            } else {
                                &i.name
                            })
                            .size(16.0)
                            .strong(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Remove").clicked() {
                                remove = Some(n);
                            }
                        });
                    });
                    ui.add_space(8.0);
                    let selected = indexer::PRESETS.iter().position(|p| p.url == i.url);
                    let mut choice = selected;
                    ui.label(RichText::new("Service").size(12.0).color(ui::MUTED));
                    egui::ComboBox::from_id_salt(("indexer-service", n))
                        .width(ui.available_width() - 16.0)
                        .selected_text(
                            selected
                                .map(|v| indexer::PRESETS[v].name)
                                .unwrap_or("Custom Newznab"),
                        )
                        .show_ui(ui, |ui| {
                            for (v, p) in indexer::PRESETS.iter().enumerate() {
                                ui.selectable_value(&mut choice, Some(v), p.name);
                            }
                            ui.selectable_value(&mut choice, None, "Custom Newznab");
                        });
                    if choice != selected {
                        *i = choice
                            .map(|v| indexer::preset_indexer(&indexer::PRESETS[v]))
                            .unwrap_or_default();
                    }
                    if choice.is_none() {
                        ui.columns(2, |c| {
                            field(&mut c[0], "Display name", &mut i.name, false);
                            field(&mut c[1], "Newznab API URL", &mut i.url, false);
                        });
                    }
                    field(ui, "API key", &mut i.api_key, true);
                    if let Some(v) = choice {
                        ui.hyperlink_to(
                            "Find your API key on the account website",
                            indexer::PRESETS[v].site,
                        );
                    }
                    diagnostics::control(
                        ui,
                        &mut self.checks,
                        n,
                        diagnostics::Service::Indexer(i.clone()),
                    );
                });
            }
            if let Some(n) = remove {
                self.config.indexers.remove(n);
            }
            if ui.button("Add indexer").clicked() {
                self.config
                    .indexers
                    .push(indexer::preset_indexer(&indexer::PRESETS[0]));
            }
        }
        ui.add_space(24.0);
        ui::section(
            ui,
            Icon::Folder,
            "Storage & processing",
            "Choose where your downloads land. Starburst takes care of the rest.",
        );
        card(ui, |ui| {
            ui.label(RichText::new("Download folder").size(12.0).color(ui::MUTED));
            ui.horizontal(|ui| {
                let path_width = (ui.available_width() - 110.0).max(120.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(path_width, 38.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.add(
                            egui::Label::new(self.config.destination.display().to_string())
                                .truncate(),
                        );
                    },
                );
                if ui.button("Browse…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        self.config.destination = p;
                    }
                }
            });
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            ui.checkbox(
                &mut self.config.unpack,
                "Automatically verify, repair and unpack",
            );
            ui.label(
                RichText::new("Original archives are kept after extraction.")
                    .size(12.0)
                    .color(ui::MUTED),
            );
        });
        ui.label(
            RichText::new("Credentials stay on this device in an owner-only settings file.")
                .size(12.0)
                .color(ui::MUTED),
        );
    }
    fn search(&mut self, ui: &mut egui::Ui) {
        if self.config.indexers.is_empty() {
            if ui::empty(
                ui,
                Icon::Search,
                "Find your next discovery",
                "Connect NZBGeek or another indexer with your API key to search from here.",
                "Connect an indexer",
            ) {
                self.page = Page::Settings;
                self.config
                    .indexers
                    .push(indexer::preset_indexer(&indexer::PRESETS[0]));
            }
            return;
        }
        let mut submit = false;
        ui.add_enabled_ui(!self.searching, |ui| {
            ui.horizontal_wrapped(|ui| {
                for &(id, label) in categories::ROOTS {
                    if ui
                        .selectable_label(self.category_root == id, label)
                        .clicked()
                        && self.category_root != id
                    {
                        self.category_root = id;
                        self.category_sub = 0;
                        submit = true;
                    }
                }
            });
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Category").size(13.0).color(ui::MUTED));
                let previous = self.category_sub;
                egui::ComboBox::from_id_salt("subcategory")
                    .width(210.0)
                    .selected_text(if self.category_sub == 0 {
                        categories::name(self.category_root)
                    } else {
                        categories::name(self.category_sub)
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.category_sub,
                            0,
                            if self.category_root == 0 {
                                "All categories".into()
                            } else {
                                format!("All {}", categories::name(self.category_root))
                            },
                        );
                        for &(id, name) in categories::children(self.category_root) {
                            ui.selectable_value(&mut self.category_sub, id, name);
                        }
                    });
                if self.category_sub != previous {
                    submit = true;
                }
                ui.label(
                    RichText::new("Leave the title blank to browse recent releases.")
                        .size(12.0)
                        .color(ui::MUTED),
                );
            });
        });
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let r = ui.add_sized(
                [ui.available_width() - 110.0, 40.0],
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text("What are you looking for?")
                    .margin(egui::vec2(12.0, 10.0)),
            );
            submit |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui
                .add_enabled(!self.searching, egui::Button::new("Search"))
                .clicked()
            {
                submit = true;
            }
        });
        if submit && !self.searching && !self.config.indexers.is_empty() {
            self.searching = true;
            self.search_finished = false;
            self.search_errors.clear();
            self.results.clear();
            let indexers = self.config.indexers.clone();
            let query = self.query.clone();
            let category = if self.category_sub == 0 {
                self.category_root
            } else {
                self.category_sub
            };
            self.searched_category = category;
            let ctx = ui.ctx().clone();
            let (tx, rx) = std::sync::mpsc::channel();
            self.search_rx = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(indexer::search_all(indexers, query, category));
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
        for error in &self.search_errors {
            card(ui, |ui| {
                ui.colored_label(Color32::LIGHT_RED, error);
            });
        }
        if self.search_finished {
            if self.results.is_empty() && !self.search_errors.is_empty() {
                ui.label(
                    "Search failed. Check the messages above and use Test API key in Settings.",
                );
            } else if self.results.is_empty() {
                ui.label("Search completed successfully, but the indexers returned no matching releases.");
            } else {
                ui.label(
                    RichText::new(format!(
                        "{} releases  /  {}",
                        self.results.len(),
                        categories::name(self.searched_category)
                    ))
                    .size(13.0)
                    .color(ui::MUTED),
                );
            }
        }
        let mut download = None;
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .id_salt("release-results")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for r in &self.results {
                    let (title, details) = ui::release_title(&r.title);
                    let width = ui.available_width();
                    egui::Frame::none()
                        .fill(ui::SURFACE)
                        .stroke(egui::Stroke::new(1.0_f32, ui::BORDER))
                        .rounding(8.0)
                        .inner_margin(egui::Margin::symmetric(18.0, 14.0))
                        .show(ui, |ui| {
                            ui.set_min_width((width - 36.0).max(0.0));
                            let bounds = ui.max_rect();
                            let button = egui::Rect::from_center_size(
                                egui::pos2(bounds.right() - 51.0, bounds.top() + 31.0),
                                egui::vec2(102.0, 36.0),
                            );
                            let text_width = (bounds.width() - 124.0).max(120.0);
                            ui.allocate_ui_with_layout(
                                egui::vec2(text_width, 62.0),
                                egui::Layout::top_down(egui::Align::LEFT),
                                |ui| {
                                    let response = ui
                                        .add(
                                            egui::Label::new(
                                                RichText::new(title).size(16.0).color(ui::TEXT),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(&r.title);
                                    response.context_menu(|ui| {
                                        if ui.button("Copy original release name").clicked() {
                                            ui.output_mut(|o| o.copied_text = r.title.clone());
                                            ui.close_menu();
                                        }
                                    });
                                    if !details.is_empty() {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(details).size(12.0).color(ui::MUTED),
                                            )
                                            .truncate(),
                                        );
                                    }
                                    let category = if r.category == 0 {
                                        self.searched_category
                                    } else {
                                        r.category
                                    };
                                    let category = if category == 0 {
                                        "Uncategorized".into()
                                    } else {
                                        categories::name(category)
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(format!(
                                                "{}   •   {}   •   {}",
                                                r.source,
                                                category,
                                                bytes(r.size)
                                            ))
                                            .size(12.0)
                                            .color(ui::MUTED),
                                        )
                                        .truncate(),
                                    );
                                },
                            );
                            if ui.put(button, egui::Button::new("Download")).clicked() {
                                download = Some((r.url.clone(), format!("{}.nzb", r.title)));
                            }
                        });
                    ui.add_space(8.0);
                }
            });
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
            let active = self
                .rows
                .iter()
                .filter(|r| r.status == Status::Downloading)
                .count();
            let speed: u64 = self
                .rows
                .iter()
                .filter(|r| r.status == Status::Downloading)
                .map(|r| r.speed)
                .sum();
            let complete = self
                .rows
                .iter()
                .filter(|r| r.status == Status::Complete)
                .count();
            ui.columns(3, |columns| {
                for (column, (title, value)) in columns.iter_mut().zip([
                    ("DOWNLOADING", active.to_string()),
                    ("TRANSFER RATE", format!("{}/s", bytes(speed))),
                    ("COMPLETED", complete.to_string()),
                ]) {
                    card(column, |ui| {
                        ui.label(RichText::new(title).size(10.0).color(ui::MUTED));
                        ui.label(RichText::new(value).size(24.0).color(ui::TEXT));
                    });
                }
            });
        }
        if !history {
            ui.horizontal(|ui| {
                if ui.button("Import NZB").clicked() {
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
                        .hint_text("Or paste an NZB download URL…")
                        .margin(egui::vec2(12.0, 10.0)),
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
                ui::icon(
                    ui,
                    if history {
                        Icon::History
                    } else {
                        Icon::Download
                    },
                    44.0,
                    ACCENT,
                );
                ui.add_space(18.0);
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
                    "Drop an NZB here, import a file, or find your next download in Discover."
                });
            });
        }
    }
}
impl eframe::App for Starburst {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(rx) = &self.search_rx {
            match rx.try_recv() {
                Ok((results, errors)) => {
                    self.notice = if errors.is_empty() {
                        format!("Found {} results", results.len())
                    } else {
                        errors.join("; ")
                    };
                    self.error = !errors.is_empty();
                    self.search_errors = errors;
                    self.search_finished = true;
                    self.results = results;
                    self.searching = false;
                    self.search_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.search_errors = vec!["Search stopped unexpectedly. Please retry.".into()];
                    self.search_finished = true;
                    self.searching = false;
                    self.search_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
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
            .exact_width(224.0)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(20, 24, 33))
                    .inner_margin(18.0),
            )
            .show(ctx, |ui| {
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(40.0, 40.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 11.0, Color32::from_rgb(61, 45, 35));
                    ui::paint_icon(ui.painter(), rect.shrink(7.0), Icon::Spark, ACCENT);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Starburst").size(21.0).strong());
                        ui.label(
                            RichText::new("USENET, SIMPLIFIED")
                                .size(9.0)
                                .color(ui::MUTED),
                        );
                    });
                });
                ui.add_space(42.0);
                ui.label(RichText::new("WORKSPACE").size(10.0).color(ui::MUTED));
                ui.add_space(6.0);
                for (page, kind, label) in [
                    (Page::Downloads, Icon::Download, "Downloads"),
                    (Page::Search, Icon::Search, "Discover"),
                    (Page::History, Icon::History, "History"),
                ] {
                    if ui::nav(ui, kind, label, self.page == page).clicked() {
                        self.page = page;
                    }
                }
                ui.add_space(24.0);
                ui.label(RichText::new("PREFERENCES").size(10.0).color(ui::MUTED));
                ui.add_space(6.0);
                if ui::nav(ui, Icon::Settings, "Settings", self.page == Page::Settings).clicked() {
                    self.page = Page::Settings;
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.label(
                        RichText::new(concat!("Starburst  /  ", env!("CARGO_PKG_VERSION")))
                            .size(11.0)
                            .color(ui::MUTED),
                    );
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(8., 8.), egui::Sense::hover());
                        ui.painter().circle_filled(
                            rect.center(),
                            3.0,
                            if self.ready {
                                Color32::from_rgb(133, 209, 167)
                            } else {
                                ACCENT
                            },
                        );
                        ui.label(
                            RichText::new(if self.ready {
                                "Engine ready"
                            } else {
                                "Ready to set up"
                            })
                            .size(12.0)
                            .color(ui::MUTED),
                        );
                    });
                    ui.separator();
                });
            });
        egui::TopBottomPanel::bottom("notice")
            .frame(
                egui::Frame::none()
                    .fill(ui::BG)
                    .inner_margin(egui::Margin::symmetric(28., 12.)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(6., 6.), egui::Sense::hover());
                    ui.painter().circle_filled(
                        rect.center(),
                        3.,
                        if self.error {
                            Color32::LIGHT_RED
                        } else {
                            ACCENT
                        },
                    );
                    ui.label(RichText::new(&self.notice).size(12.).color(if self.error {
                        Color32::LIGHT_RED
                    } else {
                        ui::MUTED
                    }));
                });
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(ui::BG)
                    .inner_margin(egui::Margin::symmetric(36., 30.)),
            )
            .show(ctx, |ui| {
                let width = ui.available_width().min(1100.0);
                let inset = (ui.available_width() - width) * 0.5;
                let content_rect = egui::Rect::from_min_size(
                    ui.cursor().min + egui::vec2(inset, 0.0),
                    egui::vec2(width, ui.available_height()),
                );
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(content_rect), |ui| {
                    ui.set_width(width);
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(match self.page {
                                    Page::Downloads => "Your downloads",
                                    Page::Search => "Discover something new",
                                    Page::History => "Download history",
                                    Page::Settings => "Settings",
                                })
                                .size(30.0)
                                .strong()
                                .color(ui::TEXT),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(match self.page {
                                    Page::Downloads => {
                                        "From your indexers to your library, without the busywork."
                                    }
                                    Page::Search => "One search. All your connected indexers.",
                                    Page::History => {
                                        "A record of everything that has come through Starburst."
                                    }
                                    Page::Settings => {
                                        "Your services, your files. Everything in its place."
                                    }
                                })
                                .color(ui::MUTED),
                            );
                        });
                        if self.page == Page::Settings {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui::primary(ui, "Save changes").clicked() {
                                        self.save_settings(ctx);
                                    }
                                },
                            );
                        }
                    });
                    ui.add_space(24.0);
                    ui.separator();
                    ui.add_space(22.0);
                    if self.page == Page::Search {
                        self.search(ui);
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt(match self.page {
                                Page::Downloads => "downloads",
                                Page::Search => "discover",
                                Page::History => "history",
                                Page::Settings => "settings",
                            })
                            .show(ui, |ui| {
                                ui.set_width(width);
                                match self.page {
                                    Page::Settings => self.settings(ui, ctx),
                                    Page::Search => self.search(ui),
                                    Page::Downloads => self.downloads(ui, false),
                                    Page::History => self.downloads(ui, true),
                                }
                            });
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
    let icon = image::load_from_memory(include_bytes!("../assets/icon.png"))?.into_rgba8();
    let (width, height) = icon.dimensions();
    let icon = egui::IconData {
        rgba: icon.into_raw(),
        width,
        height,
    };
    eframe::run_native(
        "Starburst",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_icon(icon)
                .with_app_id("io.github.soltros.Starburst")
                .with_inner_size([1120.0, 780.0])
                .with_min_inner_size([820.0, 600.0]),
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(Starburst::new(cc, config, error)))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
