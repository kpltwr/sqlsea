mod db;
mod grid;
mod sql;
mod store;
mod theme;
mod tls;

use iced::keyboard::{self, Key, key::Named};
use iced::widget::{
    Column, Row, button, column, container, operation, row, scrollable, space, svg, text,
    text_editor, text_input,
};
use iced::{Center, Element, Fill, Length, Padding, Subscription, Task};

use db::{Config, Db, Kind, Page, ResultSet, Table, Tls};
use store::Appearance;

const PAGE: u64 = 200;
const DATA_GRID: (&str, &str) = ("data-grid", "data-grid-head");
const QUERY_GRID: (&str, &str) = ("query-grid", "query-grid-head");
const STRUCTURE_GRID: (&str, &str) = ("structure-grid", "structure-grid-head");

fn main() -> iced::Result {
    let mut app = iced::application(App::boot, App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .default_font(theme::BODY)
        .window(iced::window::Settings {
            size: iced::Size::new(1280.0, 800.0),
            min_size: Some(iced::Size::new(900.0, 560.0)),
            icon: window_icon(),
            platform_specific: iced::window::settings::PlatformSpecific {
                // Matches packaging/sqlsea.desktop so launchers show the icon.
                application_id: "sqlsea".into(),
                ..Default::default()
            },
            ..Default::default()
        });
    for font in theme::FONTS {
        app = app.font(font);
    }
    app.run()
}

fn window_icon() -> Option<iced::window::Icon> {
    let decoder = png::Decoder::new(std::io::Cursor::new(
        include_bytes!("../brand/png/icon-256.png").as_slice(),
    ));
    let mut reader = decoder.read_info().ok()?;
    let mut rgba = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut rgba).ok()?;
    rgba.truncate(info.buffer_size());
    iced::window::icon::from_rgba(rgba, info.width, info.height).ok()
}

enum Load<T> {
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}

impl<T> Load<T> {
    fn ready(&self) -> Option<&T> {
        match self {
            Load::Ready(t) => Some(t),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Data,
    Structure,
    Query,
}

#[derive(Debug, Clone)]
enum Notice {
    Ok(String),
    Error(String),
}

struct App {
    saved: Vec<Config>,
    form: Config,
    /// Index into `saved` of the connection in the form, when it is a saved one.
    editing: Option<usize>,
    busy: bool,
    notice: Option<Notice>,
    session: Option<Session>,
    appearance: Appearance,
    /// What the desktop prefers, for `Appearance::Auto`.
    system_dark: bool,
}

struct Session {
    /// Whether the PostgreSQL connection is encrypted; `None` for SQLite.
    encrypted: Option<bool>,
    config: Config,
    db: Db,
    tables: Load<Vec<Table>>,
    filter: String,
    current: Option<Table>,
    tab: Tab,
    columns: Load<Columns>,
    page: Load<Page>,
    offset: u64,
    sort: Option<(usize, bool)>,
    selected: Option<usize>,
    inspector: Option<Inspector>,
    /// Bumped on every table switch or reload so late answers are dropped.
    generation: u64,
    editor: text_editor::Content,
    query: Load<ResultSet>,
    query_selected: Option<usize>,
    status: Option<Notice>,
}

/// A table's columns, with the structure listing built once when they load.
struct Columns {
    list: Vec<db::Column>,
    listing: ResultSet,
}

impl Columns {
    fn new(list: Vec<db::Column>) -> Self {
        let listing = ResultSet {
            columns: ["Column", "Type", "Nullable", "Default", "Primary key"]
                .map(String::from)
                .to_vec(),
            rows: list
                .iter()
                .map(|c| {
                    vec![
                        Some(c.name.clone()),
                        Some(c.data_type.clone()),
                        Some(if c.nullable { "yes" } else { "no" }.into()),
                        c.default.clone(),
                        Some(if c.primary_key { "yes" } else { "" }.into()),
                    ]
                })
                .collect(),
            ..Default::default()
        };
        Columns { list, listing }
    }
}

impl std::ops::Deref for Columns {
    type Target = [db::Column];

    fn deref(&self) -> &[db::Column] {
        &self.list
    }
}

struct Inspector {
    /// The page row being edited, or `None` for a new row.
    row: Option<usize>,
    fields: Vec<Field>,
    confirm_delete: bool,
    error: Option<String>,
    saving: bool,
}

struct Field {
    value: String,
    null: bool,
    original: Option<String>,
    /// Values with line breaks are edited in a multi-line editor.
    editor: Option<text_editor::Content>,
}

impl Field {
    fn new(original: Option<String>) -> Self {
        let editor = original
            .as_deref()
            .filter(|v| v.contains('\n'))
            .map(text_editor::Content::with_text);
        Field {
            value: original.clone().unwrap_or_default(),
            null: original.is_none(),
            original,
            editor,
        }
    }

    fn current(&self) -> Option<String> {
        (!self.null).then(|| self.value.clone())
    }

    fn changed(&self) -> bool {
        self.current() != self.original
    }
}

#[derive(Debug, Clone)]
enum Message {
    // Connection screen
    PickSaved(usize),
    NewConnection,
    FormKind(Kind),
    FormName(String),
    FormHost(String),
    FormPort(String),
    FormUser(String),
    FormPassword(String),
    FormDatabase(String),
    FormTls(Tls),
    FormCaFile(String),
    FormPath(String),
    Test,
    Tested(Result<Option<bool>, String>),
    Connect,
    Connected(Config, Result<db::Connected, String>),
    DeleteSaved,
    // Session
    Disconnect,
    Refresh,
    TablesLoaded(Result<Vec<Table>, String>),
    Filter(String),
    OpenTable(Table),
    #[cfg(feature = "shot")]
    OpenTableNamed(String),
    SwitchTab(Tab),
    ColumnsLoaded(u64, Result<Vec<db::Column>, String>),
    PageLoaded(u64, Result<Page, String>),
    Reload,
    Sort(usize),
    PrevPage,
    NextPage,
    SelectRow(usize),
    AddRow,
    CloseInspector,
    FieldInput(usize, String),
    FieldEditor(usize, text_editor::Action),
    FieldNull(usize),
    CancelDelete,
    SaveRow,
    DiscardRow,
    DeleteRow,
    RowWritten(Result<String, String>),
    Edit(text_editor::Action),
    #[cfg(feature = "shot")]
    SetSql(String),
    Run,
    QueryDone(u64, Result<ResultSet, String>),
    SelectQueryRow(usize),
    GridScrolled(&'static str, f32),
    Key(keyboard::Event),
    SetAppearance(Appearance),
    SystemTheme(iced::theme::Mode),
    #[cfg(feature = "shot")]
    Shoot(String),
}

impl App {
    fn boot() -> (Self, Task<Message>) {
        let (saved, keyring_problem) = store::load();
        let mut app = App {
            form: Config::default(),
            editing: None,
            busy: false,
            notice: keyring_problem.map(Notice::Error),
            session: None,
            saved,
            appearance: store::load_settings().appearance,
            system_dark: false,
        };
        let mut task = iced::system::theme().map(Message::SystemTheme);
        // `sqlsea app.db` opens a SQLite file; `sqlsea name` opens a saved connection.
        if let Some(arg) = std::env::args().nth(1) {
            if let Some(i) = app.saved.iter().position(|c| c.name == arg) {
                app.form = app.saved[i].clone();
                app.editing = Some(i);
            } else {
                app.form = Config {
                    name: file_stem(&arg),
                    kind: Kind::Sqlite,
                    path: arg,
                    ..Default::default()
                };
            }
            app.busy = true;
            task = Task::batch([task, connect(app.form.clone())]);
        }
        #[cfg(feature = "shot")]
        {
            task = task.chain(script());
        }
        (app, task)
    }

    fn title(&self) -> String {
        match &self.session {
            Some(s) => format!("{} – sqlsea", s.config.name),
            None => "sqlsea".into(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            keyboard::listen().map(Message::Key),
            iced::system::theme_changes().map(Message::SystemTheme),
        ])
    }

    fn theme(&self) -> iced::Theme {
        theme::app_theme(match self.appearance {
            Appearance::Auto => self.system_dark,
            Appearance::Light => false,
            Appearance::Dark => true,
        })
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PickSaved(i) => {
                self.form = self.saved[i].clone();
                self.editing = Some(i);
                self.notice = None;
            }
            Message::NewConnection => {
                self.form = Config::default();
                self.editing = None;
                self.notice = None;
            }
            Message::FormKind(kind) => self.form.kind = kind,
            Message::FormName(v) => self.form.name = v,
            Message::FormHost(v) => self.form.host = v,
            Message::FormPort(v) => self.form.port = v,
            Message::FormUser(v) => self.form.user = v,
            Message::FormPassword(v) => self.form.password = v,
            Message::FormDatabase(v) => self.form.database = v,
            Message::FormTls(v) => self.form.tls = v,
            Message::FormCaFile(v) => self.form.ca_file = v,
            Message::FormPath(v) => self.form.path = v,
            Message::Test => {
                self.busy = true;
                self.notice = None;
                let config = normalized(&self.form);
                return Task::perform(
                    async move { db::connect(config).await.map(|c| c.encrypted) },
                    Message::Tested,
                );
            }
            Message::Tested(result) => {
                self.busy = false;
                self.notice = Some(match result {
                    Ok(Some(true)) => Notice::Ok("Connected. The connection is encrypted.".into()),
                    Ok(Some(false)) => Notice::Ok("Connected, without encryption.".into()),
                    Ok(None) => Notice::Ok("Connected. The details work.".into()),
                    Err(e) => Notice::Error(e),
                });
            }
            Message::Connect => {
                self.busy = true;
                self.notice = None;
                let mut config = normalized(&self.form);
                if config.name.trim().is_empty() {
                    config.name = match config.kind {
                        Kind::Postgres => config.target(),
                        Kind::Sqlite => file_stem(&config.path),
                    };
                }
                if config.id.is_empty() {
                    config.id = store::new_id();
                }
                self.form = config.clone();
                return connect(config);
            }
            Message::Connected(config, result) => {
                self.busy = false;
                match result {
                    Ok(connected) => {
                        self.remember(&config);
                        let mut session = Session::new(config, connected);
                        let task = session.load_tables();
                        self.session = Some(session);
                        return task;
                    }
                    Err(e) => self.notice = Some(Notice::Error(e)),
                }
            }
            Message::DeleteSaved => {
                if let Some(i) = self.editing.take() {
                    let gone = self.saved.remove(i);
                    self.form = Config::default();
                    self.notice = store::save(&self.saved)
                        .and_then(|()| store::delete_password(&gone.id))
                        .err()
                        .map(Notice::Error);
                }
            }
            Message::Disconnect => {
                self.session = None;
                self.notice = None;
            }
            Message::Key(event) => return self.on_key(event),
            Message::SetAppearance(appearance) => {
                self.appearance = appearance;
                if let Err(e) = store::save_settings(&store::Settings { appearance }) {
                    self.notice = Some(Notice::Error(format!("Could not save the setting: {e}")));
                }
            }
            Message::SystemTheme(mode) => self.system_dark = mode == iced::theme::Mode::Dark,
            Message::GridScrolled(head, x) => {
                return operation::scroll_to(
                    head,
                    operation::AbsoluteOffset {
                        x: Some(x),
                        y: None,
                    },
                );
            }
            #[cfg(feature = "shot")]
            Message::Shoot(path) => {
                return iced::window::latest()
                    .and_then(iced::window::screenshot)
                    .map(move |shot| {
                        save_png(&path, &shot);
                        Message::Refresh
                    })
                    .discard();
            }
            other => {
                if let Some(session) = &mut self.session {
                    return session.update(other);
                }
            }
        }
        Task::none()
    }

    /// Put a connection that worked at the top of the saved list.
    fn remember(&mut self, config: &Config) {
        if let Some(i) = self.editing.take() {
            self.saved.remove(i);
        }
        // A different connection with the same name is replaced, password and all.
        for old in self
            .saved
            .iter()
            .filter(|c| c.name == config.name && c.id != config.id)
        {
            let _ = store::delete_password(&old.id);
        }
        self.saved.retain(|c| c.name != config.name);
        self.saved.insert(0, config.clone());
        self.editing = Some(0);
        if let Err(e) = store::save(&self.saved) {
            self.notice = Some(Notice::Error(format!("Could not save the connection: {e}")));
        } else if let Err(e) = store::set_password(&config.id, &config.password) {
            self.notice = Some(Notice::Error(format!(
                "Connected, but the password could not go into the system keyring, \
                 so it was not saved.\n{e}"
            )));
        }
    }

    fn on_key(&mut self, event: keyboard::Event) -> Task<Message> {
        let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
            return Task::none();
        };
        match key.as_ref() {
            Key::Named(Named::Tab) if modifiers.shift() => operation::focus_previous(),
            Key::Named(Named::Tab) => operation::focus_next(),
            Key::Named(Named::Enter) if self.session.is_none() && !self.busy => {
                self.update(Message::Connect)
            }
            _ => match &mut self.session {
                Some(session) => session.on_key(key, modifiers),
                None => Task::none(),
            },
        }
    }

    fn view(&self) -> Element<'_, Message> {
        match &self.session {
            Some(session) => session.view(self.appearance),
            None => self.connect_view(),
        }
    }

    fn connect_view(&self) -> Element<'_, Message> {
        let list: Element<_> = if self.saved.is_empty() {
            text("No saved connections yet. Fill in the form to add your first one.")
                .size(theme::TEXT_MD)
                .style(theme::text_muted)
                .into()
        } else {
            Column::with_children(self.saved.iter().enumerate().map(|(i, c)| {
                button(
                    column![
                        text(c.name.as_str())
                            .font(theme::BODY_MEDIUM)
                            .size(theme::TEXT_MD),
                        text(c.target())
                            .font(theme::MONO)
                            .size(theme::TEXT_XS)
                            .style(theme::text_muted)
                            .wrapping(text::Wrapping::None),
                    ]
                    .spacing(2),
                )
                .width(Fill)
                .padding(Padding::from([8.0, 10.0]))
                .style(theme::list_row(self.editing == Some(i)))
                .on_press(Message::PickSaved(i))
                .into()
            }))
            .spacing(2)
            .into()
        };

        let saved = column![
            row![
                text("Connections")
                    .font(theme::DISPLAY)
                    .size(theme::TEXT_XL),
                space::horizontal(),
                small_key("New", theme::key, Some(Message::NewConnection)),
            ]
            .align_y(Center),
            scrollable(list).style(theme::scroller).height(Fill),
        ]
        .spacing(16)
        .width(300);

        let f = &self.form;
        let kind = container(
            row![
                kind_key(Kind::Postgres, f.kind),
                kind_key(Kind::Sqlite, f.kind),
            ]
            .spacing(2),
        )
        .padding(3)
        .style(theme::tray);

        let mut fields = column![labelled(
            "Name",
            input("My database", &f.name, Message::FormName)
        )]
        .spacing(14);
        fields = match f.kind {
            Kind::Postgres => fields
                .push(
                    row![
                        labelled("Host", input("localhost", &f.host, Message::FormHost)),
                        container(labelled("Port", input("5432", &f.port, Message::FormPort)))
                            .width(110),
                    ]
                    .spacing(12),
                )
                .push(
                    row![
                        labelled("User", input("postgres", &f.user, Message::FormUser)),
                        labelled(
                            "Password",
                            input(
                                "Kept in the system keyring",
                                &f.password,
                                Message::FormPassword
                            )
                            .secure(true)
                        ),
                    ]
                    .spacing(12),
                )
                .push(labelled(
                    "Database",
                    input("Same as the user", &f.database, Message::FormDatabase),
                ))
                .push(labelled(
                    "Encryption",
                    column![
                        container(
                            Row::with_children(Tls::ALL.map(|mode| {
                                button(
                                    text(mode.label())
                                        .font(theme::BODY_MEDIUM)
                                        .size(theme::TEXT_SM),
                                )
                                .padding(Padding::from([6.0, 12.0]))
                                .style(theme::tray_key(mode == f.tls))
                                .on_press(Message::FormTls(mode))
                                .into()
                            }))
                            .spacing(2),
                        )
                        .padding(3)
                        .style(theme::tray),
                        muted(f.tls.help()),
                    ]
                    .spacing(8),
                ))
                .push((f.tls == Tls::Verify).then(|| {
                    labelled(
                        "CA certificate file",
                        input(
                            "Optional. A PEM file from your provider",
                            &f.ca_file,
                            Message::FormCaFile,
                        )
                        .font(theme::MONO),
                    )
                })),
            Kind::Sqlite => fields.push(labelled(
                "Database file",
                input("~/data/app.db", &f.path, Message::FormPath).font(theme::MONO),
            )),
        };

        let note: Element<_> = match &self.notice {
            Some(Notice::Error(e)) => error_note(e),
            Some(Notice::Ok(m)) => text(m.as_str())
                .size(theme::TEXT_SM)
                .style(theme::text_ok)
                .into(),
            None => space().into(),
        };

        let busy = self.busy;
        let mut actions = row![
            small_key(
                if busy { "Connecting" } else { "Connect" },
                theme::key_primary,
                (!busy).then_some(Message::Connect)
            ),
            small_key("Test", theme::key, (!busy).then_some(Message::Test)),
            space::horizontal(),
        ]
        .spacing(8);
        if self.editing.is_some() {
            actions = actions.push(small_key(
                "Delete",
                theme::key_danger,
                (!busy).then_some(Message::DeleteSaved),
            ));
        }

        let form = container(
            column![
                text(if self.editing.is_some() {
                    "Connection"
                } else {
                    "New connection"
                })
                .font(theme::DISPLAY)
                .size(theme::TEXT_LG),
                kind,
                fields,
                note,
                actions,
            ]
            .spacing(18),
        )
        .padding(24)
        .width(480)
        .style(theme::panel);
        // On short windows the form scrolls rather than spilling off the bottom.
        let form = scrollable(form).style(theme::scroller);

        let body = container(row![saved, form].spacing(40))
            .center_x(Fill)
            .padding(Padding::from([32.0, 24.0]))
            .height(Fill);

        column![top_bar(row![], None, self.appearance), hrule(), body].into()
    }
}

impl Session {
    fn new(config: Config, connected: db::Connected) -> Self {
        Session {
            config,
            db: connected.db,
            encrypted: connected.encrypted,
            tables: Load::Loading,
            filter: String::new(),
            current: None,
            tab: Tab::Data,
            columns: Load::Idle,
            page: Load::Idle,
            offset: 0,
            sort: None,
            selected: None,
            inspector: None,
            generation: 0,
            editor: text_editor::Content::new(),
            query: Load::Idle,
            query_selected: None,
            status: None,
        }
    }

    fn load_tables(&mut self) -> Task<Message> {
        self.tables = Load::Loading;
        Task::perform(db::tables(self.db.clone()), Message::TablesLoaded)
    }

    fn open(&mut self, table: Table) -> Task<Message> {
        self.current = Some(table);
        self.offset = 0;
        self.sort = None;
        self.selected = None;
        self.inspector = None;
        if self.tab == Tab::Query {
            self.tab = Tab::Data;
        }
        self.load_columns()
    }

    fn load_columns(&mut self) -> Task<Message> {
        let Some(table) = self.current.clone() else {
            return Task::none();
        };
        self.generation += 1;
        let generation = self.generation;
        self.columns = Load::Loading;
        self.page = Load::Loading;
        Task::perform(db::columns(self.db.clone(), table), move |r| {
            Message::ColumnsLoaded(generation, r)
        })
    }

    fn load_page(&mut self) -> Task<Message> {
        let (Some(table), Some(columns)) = (self.current.clone(), self.columns.ready()) else {
            return Task::none();
        };
        // Page by the primary key unless a column head chose otherwise, so rows stay put.
        let order = match self.sort {
            Some((i, asc)) => columns.get(i).map(|c| (c.name.clone(), asc)),
            None => columns
                .iter()
                .find(|c| c.primary_key)
                .map(|c| (c.name.clone(), true)),
        };
        self.generation += 1;
        let generation = self.generation;
        self.page = Load::Loading;
        self.selected = None;
        self.inspector = None;
        let db = self.db.clone();
        let offset = self.offset;
        Task::perform(db::page(db, table, order, offset, PAGE), move |r| {
            Message::PageLoaded(generation, r)
        })
        .chain(operation::snap_to(
            DATA_GRID.0,
            operation::RelativeOffset::START,
        ))
    }

    fn editable(&self) -> bool {
        let has_key = self
            .columns
            .ready()
            .is_some_and(|c| c.iter().any(|c| c.primary_key));
        has_key && self.current.as_ref().is_some_and(|t| !t.is_view)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Refresh => {
                let tables = self.load_tables();
                return match self.current {
                    Some(_) => Task::batch([tables, self.load_columns()]),
                    None => tables,
                };
            }
            Message::TablesLoaded(result) => match result {
                Ok(tables) => {
                    // Keep the open table if it still exists; otherwise open the first.
                    let keep = self.current.as_ref().is_some_and(|t| tables.contains(t));
                    let first = tables.first().cloned();
                    self.tables = Load::Ready(tables);
                    if !keep {
                        self.current = None;
                        if let Some(first) = first.filter(|_| self.tab != Tab::Query) {
                            return self.open(first);
                        }
                    }
                }
                Err(e) => self.tables = Load::Failed(e),
            },
            Message::Filter(f) => self.filter = f,
            Message::OpenTable(table) => return self.open(table),
            #[cfg(feature = "shot")]
            Message::OpenTableNamed(name) => {
                let found = self
                    .tables
                    .ready()
                    .and_then(|t| t.iter().find(|t| t.name == name).cloned());
                if let Some(table) = found {
                    return self.open(table);
                }
            }
            Message::SwitchTab(tab) => {
                self.tab = tab;
                if tab == Tab::Query {
                    return operation::focus("sql");
                }
            }
            Message::ColumnsLoaded(generation, result) => {
                if generation != self.generation {
                    return Task::none();
                }
                match result {
                    Ok(columns) => {
                        self.columns = Load::Ready(Columns::new(columns));
                        return self.load_page();
                    }
                    Err(e) => {
                        self.columns = Load::Failed(e.clone());
                        self.page = Load::Failed(e);
                    }
                }
            }
            Message::PageLoaded(generation, result) => {
                if generation == self.generation {
                    self.page = match result {
                        Ok(page) => Load::Ready(page),
                        Err(e) => Load::Failed(e),
                    };
                }
            }
            Message::Reload => return self.load_columns(),
            Message::Sort(i) => {
                self.sort = match self.sort {
                    Some((c, true)) if c == i => Some((i, false)),
                    Some((c, false)) if c == i => None,
                    _ => Some((i, true)),
                };
                self.offset = 0;
                return self.load_page();
            }
            Message::PrevPage => {
                self.offset = self.offset.saturating_sub(PAGE);
                return self.load_page();
            }
            Message::NextPage => {
                self.offset += PAGE;
                return self.load_page();
            }
            Message::SelectRow(r) => {
                let (Some(page), Some(columns)) = (self.page.ready(), self.columns.ready()) else {
                    return Task::none();
                };
                let Some(values) = page.result.rows.get(r) else {
                    return Task::none();
                };
                self.selected = Some(r);
                self.inspector = Some(Inspector {
                    row: Some(r),
                    fields: values
                        .iter()
                        .take(columns.len())
                        .map(|v| Field::new(v.clone()))
                        .collect(),
                    confirm_delete: false,
                    error: None,
                    saving: false,
                });
            }
            Message::AddRow => {
                let Some(columns) = self.columns.ready() else {
                    return Task::none();
                };
                self.selected = None;
                self.inspector = Some(Inspector {
                    row: None,
                    fields: columns.iter().map(|_| Field::new(None)).collect(),
                    confirm_delete: false,
                    error: None,
                    saving: false,
                });
            }
            Message::CloseInspector => {
                self.inspector = None;
                self.selected = None;
            }
            Message::FieldInput(i, v) => {
                if let Some(ins) = &mut self.inspector {
                    ins.error = None;
                }
                if let Some(field) = self
                    .inspector
                    .as_mut()
                    .and_then(|ins| ins.fields.get_mut(i))
                {
                    field.value = v;
                    field.null = false;
                }
            }
            Message::FieldEditor(i, action) => {
                if let Some(field) = self
                    .inspector
                    .as_mut()
                    .and_then(|ins| ins.fields.get_mut(i))
                    && let Some(editor) = &mut field.editor
                {
                    let edit = action.is_edit();
                    editor.perform(action);
                    if edit {
                        field.value = editor.text();
                        field.null = false;
                    }
                }
            }
            Message::FieldNull(i) => {
                if let Some(field) = self
                    .inspector
                    .as_mut()
                    .and_then(|ins| ins.fields.get_mut(i))
                {
                    field.null = !field.null;
                }
            }
            Message::DiscardRow => {
                if let Some(ins) = &mut self.inspector {
                    for field in &mut ins.fields {
                        *field = Field::new(field.original.clone());
                    }
                    ins.confirm_delete = false;
                }
            }
            Message::CancelDelete => {
                if let Some(ins) = &mut self.inspector {
                    ins.confirm_delete = false;
                }
            }
            Message::SaveRow => return self.save_row(),
            Message::DeleteRow => return self.delete_row(),
            Message::RowWritten(result) => match result {
                Ok(done) => {
                    self.status = Some(Notice::Ok(done));
                    return self.load_page();
                }
                Err(e) => {
                    if let Some(ins) = &mut self.inspector {
                        ins.saving = false;
                        ins.confirm_delete = false;
                        ins.error = Some(explain(&e));
                    }
                }
            },
            Message::Edit(action) => self.editor.perform(action),
            #[cfg(feature = "shot")]
            Message::SetSql(sql) => {
                self.editor.perform(text_editor::Action::SelectAll);
                self.editor
                    .perform(text_editor::Action::Edit(text_editor::Edit::Paste(
                        std::sync::Arc::new(sql),
                    )));
            }
            Message::Run => return self.run(),
            Message::QueryDone(generation, result) => {
                if generation != self.generation {
                    return Task::none();
                }
                let changed_schema = matches!(&result, Ok(rs) if rs.columns.is_empty());
                self.query = match result {
                    Ok(rs) => Load::Ready(rs),
                    Err(e) => Load::Failed(e),
                };
                // A statement without a result set may have created or dropped a table.
                if changed_schema {
                    return self.load_tables();
                }
            }
            Message::SelectQueryRow(r) => self.query_selected = Some(r),
            _ => {}
        }
        Task::none()
    }

    fn save_row(&mut self) -> Task<Message> {
        let (Some(ins), Some(table), Some(columns)) = (
            self.inspector.as_mut(),
            self.current.clone(),
            self.columns.ready(),
        ) else {
            return Task::none();
        };
        let changes: Vec<(usize, Option<String>)> = ins
            .fields
            .iter()
            .enumerate()
            .filter(|(_, f)| f.changed())
            .map(|(i, f)| (i, f.current()))
            .collect();
        if changes.is_empty() {
            return Task::none();
        }
        let (sql, done) = match ins.row {
            Some(_) => {
                let original: Vec<Option<String>> =
                    ins.fields.iter().map(|f| f.original.clone()).collect();
                match db::update_sql(&table, columns, &original, &changes) {
                    Some(sql) => (sql, format!("Updated a row in {}.", table.name)),
                    None => return Task::none(),
                }
            }
            None => (
                db::insert_sql(&table, columns, &changes),
                format!("Added a row to {}.", table.name),
            ),
        };
        ins.saving = true;
        write(self.db.clone(), sql, done)
    }

    fn delete_row(&mut self) -> Task<Message> {
        let (Some(ins), Some(table), Some(columns)) = (
            self.inspector.as_mut(),
            self.current.clone(),
            self.columns.ready(),
        ) else {
            return Task::none();
        };
        if !ins.confirm_delete {
            ins.confirm_delete = true;
            return Task::none();
        }
        let original: Vec<Option<String>> = ins.fields.iter().map(|f| f.original.clone()).collect();
        let Some(sql) = db::delete_sql(&table, columns, &original) else {
            return Task::none();
        };
        ins.saving = true;
        write(
            self.db.clone(),
            sql,
            format!("Deleted a row from {}.", table.name),
        )
    }

    fn run(&mut self) -> Task<Message> {
        let sql = self
            .editor
            .selection()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| self.editor.text());
        if sql.trim().is_empty() {
            return Task::none();
        }
        self.tab = Tab::Query;
        self.generation += 1;
        let generation = self.generation;
        self.query = Load::Loading;
        self.query_selected = None;
        Task::perform(db::execute(self.db.clone(), sql), move |r| {
            Message::QueryDone(generation, r)
        })
    }

    fn on_key(&mut self, key: Key, modifiers: keyboard::Modifiers) -> Task<Message> {
        match key.as_ref() {
            Key::Named(Named::Escape) if self.inspector.is_some() => {
                self.update(Message::CloseInspector)
            }
            Key::Named(Named::Enter) if modifiers.command() && self.tab == Tab::Query => self.run(),
            Key::Character("r") if modifiers.command() => self.update(Message::Refresh),
            Key::Character("1") if modifiers.command() => {
                self.update(Message::SwitchTab(Tab::Data))
            }
            Key::Character("2") if modifiers.command() => {
                self.update(Message::SwitchTab(Tab::Structure))
            }
            Key::Character("3") if modifiers.command() => {
                self.update(Message::SwitchTab(Tab::Query))
            }
            _ => Task::none(),
        }
    }

    // ---------- view ----------

    fn view(&self, appearance: Appearance) -> Element<'_, Message> {
        let bar = top_bar(
            row![
                vrule_short(),
                text(self.config.name.as_str())
                    .font(theme::BODY_MEDIUM)
                    .size(theme::TEXT_MD),
                text(self.config.target())
                    .font(theme::MONO)
                    .size(theme::TEXT_XS)
                    .style(theme::text_muted),
                text(match self.encrypted {
                    Some(true) => "Encrypted",
                    Some(false) => "Not encrypted",
                    None => "",
                })
                .size(theme::TEXT_XS)
                .style(theme::text_muted),
            ]
            .spacing(12)
            .align_y(Center),
            Some(
                row![
                    small_key("Refresh", theme::key, Some(Message::Refresh)),
                    small_key("Disconnect", theme::key, Some(Message::Disconnect)),
                ]
                .spacing(12)
                .into(),
            ),
            appearance,
        );

        let mut middle = row![
            container(self.sidebar())
                .width(260)
                .height(Fill)
                .style(theme::sidebar),
            vrule(),
            container(self.main()).width(Fill).height(Fill),
        ];
        if self.tab == Tab::Data
            && let Some(ins) = &self.inspector
        {
            middle = middle.push(vrule()).push(self.inspector_view(ins));
        }

        column![
            bar,
            hrule(),
            middle.height(Fill),
            hrule(),
            self.status_bar()
        ]
        .into()
    }

    fn sidebar(&self) -> Element<'_, Message> {
        let filter = text_input("Filter tables", &self.filter)
            .on_input(Message::Filter)
            .padding(Padding::from([6.0, 10.0]))
            .size(theme::TEXT_MD)
            .style(theme::field);

        let list: Element<_> = match &self.tables {
            Load::Idle | Load::Loading => muted("Reading the schema…"),
            Load::Failed(e) => column![
                error_note(e),
                small_key("Try again", theme::key, Some(Message::Refresh))
            ]
            .spacing(10)
            .into(),
            Load::Ready(tables) if tables.is_empty() => {
                muted("No tables yet. Create one from the Query tab.")
            }
            Load::Ready(tables) => {
                let needle = self.filter.trim().to_lowercase();
                let shown: Vec<&Table> = tables
                    .iter()
                    .filter(|t| needle.is_empty() || t.name.to_lowercase().contains(&needle))
                    .collect();
                if shown.is_empty() {
                    muted(format!("No table matches “{}”.", self.filter.trim()))
                } else {
                    let many_schemas = tables.iter().any(|t| t.schema != tables[0].schema);
                    let mut list = Column::new().spacing(1);
                    let mut last_schema: Option<&Option<String>> = None;
                    for t in shown {
                        if many_schemas && last_schema != Some(&t.schema) {
                            list = list.push(
                                container(
                                    text(t.schema.as_deref().unwrap_or(""))
                                        .font(theme::LABEL)
                                        .size(theme::TEXT_XS)
                                        .style(theme::text_muted),
                                )
                                .padding(Padding::from([10.0, 10.0]).bottom(4.0)),
                            );
                            last_schema = Some(&t.schema);
                        }
                        let selected = self.current.as_ref() == Some(t);
                        let mut label = row![
                            text(t.name.as_str())
                                .size(theme::TEXT_MD)
                                .wrapping(text::Wrapping::None)
                        ]
                        .align_y(Center);
                        if t.is_view {
                            label = label
                                .push(space::horizontal())
                                .push(text("view").size(theme::TEXT_XS).style(theme::text_muted));
                        }
                        list = list.push(
                            button(label)
                                .width(Fill)
                                .padding(Padding::from([5.0, 10.0]))
                                .style(theme::list_row(selected))
                                .on_press(Message::OpenTable(t.clone())),
                        );
                    }
                    scrollable(list).style(theme::scroller).height(Fill).into()
                }
            }
        };

        column![filter, list].spacing(12).padding(12).into()
    }

    fn main(&self) -> Element<'_, Message> {
        let tab = |label: &'static str, which: Tab| {
            let active = self.tab == which;
            column![
                button(
                    text(label)
                        .font(theme::BODY_MEDIUM)
                        .size(theme::TEXT_MD)
                        .style(move |t| if active {
                            theme::text_ink(t)
                        } else {
                            theme::text_muted(t)
                        })
                )
                .padding(Padding::from([6.0, 10.0]))
                .style(theme::ghost)
                .on_press(Message::SwitchTab(which)),
                container(space().height(2)).width(Fill).style(if active {
                    theme::accent_bar
                } else {
                    theme::bar
                }),
            ]
            .spacing(6)
            .width(Length::Shrink)
        };

        let tools: Element<_> = match self.tab {
            Tab::Data => self.data_tools(),
            Tab::Structure => space().into(),
            Tab::Query => row![
                muted("Ctrl+Enter"),
                small_key("Run", theme::key_primary, Some(Message::Run)),
            ]
            .spacing(12)
            .align_y(Center)
            .into(),
        };

        let strip = container(
            row![
                tab("Data", Tab::Data),
                tab("Structure", Tab::Structure),
                tab("Query", Tab::Query),
                space::horizontal(),
                container(tools).padding(Padding::ZERO.bottom(6.0)),
            ]
            .spacing(4)
            .align_y(iced::Bottom),
        )
        .padding(Padding::from([0.0, 12.0]).top(8.0))
        .width(Fill);

        let content = match self.tab {
            Tab::Data => self.data_view(),
            Tab::Structure => self.structure_view(),
            Tab::Query => self.query_view(),
        };

        column![strip, hrule(), content].into()
    }

    fn data_tools(&self) -> Element<'_, Message> {
        let Some(page) = self.page.ready() else {
            return space().into();
        };
        let shown = page.result.rows.len() as u64;
        let range = if shown == 0 {
            format!("0 of {}", thousands(page.total))
        } else {
            format!(
                "{}–{} of {}",
                thousands(self.offset + 1),
                thousands(self.offset + shown),
                thousands(page.total)
            )
        };
        let can_add = self.current.as_ref().is_some_and(|t| !t.is_view);
        row![
            muted(range),
            small_key(
                "Previous",
                theme::key,
                (self.offset > 0).then_some(Message::PrevPage)
            ),
            small_key(
                "Next",
                theme::key,
                (self.offset + shown < page.total).then_some(Message::NextPage)
            ),
            small_key("Add row", theme::key, can_add.then_some(Message::AddRow)),
        ]
        .spacing(8)
        .align_y(Center)
        .into()
    }

    fn data_view(&self) -> Element<'_, Message> {
        let Some(table) = &self.current else {
            return empty_state("Choose a table on the left to see its rows.", None);
        };
        match &self.page {
            Load::Idle | Load::Loading => grid::skeleton(),
            Load::Failed(e) => empty_state_error(e),
            Load::Ready(page) if page.result.rows.is_empty() && self.offset == 0 => empty_state(
                format!("{} has no rows yet.", table.name),
                (!table.is_view).then(|| small_key("Add row", theme::key, Some(Message::AddRow))),
            ),
            Load::Ready(page) => grid::view(grid::Grid {
                result: &page.result,
                selected: self.selected,
                on_select: Some(Message::SelectRow),
                on_sort: Some(Message::Sort),
                sort: self.sort,
                ids: DATA_GRID,
                on_scroll: Message::GridScrolled,
            }),
        }
    }

    fn structure_view(&self) -> Element<'_, Message> {
        if self.current.is_none() {
            return empty_state("Choose a table on the left to see its columns.", None);
        }
        match &self.columns {
            Load::Idle | Load::Loading => grid::skeleton(),
            Load::Failed(e) => empty_state_error(e),
            Load::Ready(columns) => grid::view(grid::Grid {
                result: &columns.listing,
                selected: None,
                on_select: None,
                on_sort: None,
                sort: None,
                ids: STRUCTURE_GRID,
                on_scroll: Message::GridScrolled,
            }),
        }
    }

    fn query_view(&self) -> Element<'_, Message> {
        let editor = text_editor(&self.editor)
            .id("sql")
            .placeholder("select * from …")
            .on_action(Message::Edit)
            .font(theme::MONO)
            .size(theme::TEXT_MD)
            .padding(16)
            .height(Fill)
            .highlight_with::<sql::Sql>((), sql::format)
            .key_binding(|press| {
                if matches!(press.key.as_ref(), Key::Named(Named::Enter))
                    && press.modifiers.command()
                {
                    return Some(text_editor::Binding::Custom(Message::Run));
                }
                if matches!(press.key.as_ref(), Key::Named(Named::Tab)) && !press.modifiers.shift()
                {
                    return Some(text_editor::Binding::Sequence(vec![
                        text_editor::Binding::Insert(' '),
                        text_editor::Binding::Insert(' '),
                    ]));
                }
                text_editor::Binding::from_key_press(press)
            })
            .style(theme::sql_editor);

        let results: Element<_> = match &self.query {
            Load::Idle => empty_state(
                "Results show here. Write SQL above and press Ctrl+Enter; a selection runs on its own.",
                None,
            ),
            Load::Loading => grid::skeleton(),
            Load::Failed(e) => container(error_note(e)).padding(16).width(Fill).into(),
            Load::Ready(rs) if rs.columns.is_empty() => {
                let mut note = column![
                    text(match rs.affected {
                        Some(n) => format!("{} affected", count(n, "row")),
                        None => "The statement ran.".to_string(),
                    })
                    .font(theme::DISPLAY)
                    .size(theme::TEXT_LG)
                ]
                .spacing(4);
                if rs.affected == Some(0) {
                    note = note.push(muted("Nothing matched the statement's conditions."));
                }
                container(note).padding(16).width(Fill).into()
            }
            Load::Ready(rs) => grid::view(grid::Grid {
                result: rs,
                selected: self.query_selected,
                on_select: Some(Message::SelectQueryRow),
                on_sort: None,
                sort: None,
                ids: QUERY_GRID,
                on_scroll: Message::GridScrolled,
            }),
        };

        column![
            container(editor)
                .height(Length::FillPortion(2))
                .style(theme::graphite),
            container(results).height(Length::FillPortion(3)),
        ]
        .into()
    }

    fn inspector_view<'a>(&'a self, ins: &'a Inspector) -> Element<'a, Message> {
        let Some(columns) = self.columns.ready() else {
            return space().into();
        };
        let editable = ins.row.is_none() || self.editable();
        let title = match ins.row {
            Some(r) => format!("Row {}", thousands(self.offset + r as u64 + 1)),
            None => "New row".into(),
        };

        let head = row![
            column![
                text(title).font(theme::DISPLAY).size(theme::TEXT_LG),
                text(self.current.as_ref().map(|t| t.name.as_str()).unwrap_or(""))
                    .font(theme::MONO)
                    .size(theme::TEXT_XS)
                    .style(theme::text_muted),
            ]
            .spacing(2),
            space::horizontal(),
            small_key("Close", theme::ghost, Some(Message::CloseInspector)),
        ]
        .align_y(Center);

        let fields = Column::with_children(columns.iter().zip(&ins.fields).enumerate().map(
            |(i, (col, field))| {
                let mut label = row![
                    text(col.name.as_str())
                        .font(theme::LABEL)
                        .size(theme::TEXT_SM)
                        .style(theme::text_ink2),
                    text(col.data_type.as_str())
                        .font(theme::MONO)
                        .size(theme::TEXT_XS)
                        .style(theme::text_muted),
                ]
                .spacing(8)
                .align_y(Center);
                if col.primary_key {
                    label = label.push(text("key").size(theme::TEXT_XS).style(theme::text_muted));
                }
                label = label.push(space::horizontal());
                if editable && col.nullable {
                    label = label.push(
                        button(text("NULL").font(theme::MONO).size(11))
                            .padding(Padding::from([1.0, 6.0]))
                            .style(theme::null_toggle(field.null))
                            .on_press(Message::FieldNull(i)),
                    );
                }
                let placeholder = match (ins.row, field.null) {
                    (None, true) => col.default.as_deref().map_or("default", |_| "default"),
                    (_, true) => "NULL",
                    _ => "",
                };
                let style = if field.changed() {
                    theme::field_changed
                } else {
                    theme::field
                };
                if let Some(editor) = &field.editor {
                    let mut input = text_editor(editor)
                        .font(theme::MONO)
                        .size(theme::TEXT_SM)
                        .padding(Padding::from([7.0, 10.0]))
                        .height(120)
                        .style(move |t, status| theme::as_editor(style, t, status));
                    if editable {
                        input = input.on_action(move |a| Message::FieldEditor(i, a));
                    }
                    return column![label, input].spacing(6).into();
                }
                let value = if field.null { "" } else { field.value.as_str() };
                let input = text_input(placeholder, value)
                    .font(theme::MONO)
                    .size(theme::TEXT_SM)
                    .padding(Padding::from([7.0, 10.0]))
                    .on_input_maybe(editable.then_some(move |v| Message::FieldInput(i, v)))
                    .style(style);
                column![label, input].spacing(6).into()
            },
        ))
        .spacing(14);

        let changed = ins.fields.iter().any(Field::changed);
        let footer: Element<_> = if !editable {
            muted(match self.current.as_ref().is_some_and(|t| t.is_view) {
                true => "Views are read-only here.",
                false => "This table has no primary key, so its rows are read-only here.",
            })
        } else {
            let mut actions = row![
                small_key(
                    if ins.saving {
                        "Saving"
                    } else if ins.row.is_some() {
                        "Save"
                    } else {
                        "Insert"
                    },
                    theme::key_primary,
                    (changed && !ins.saving).then_some(Message::SaveRow)
                ),
                small_key(
                    "Discard",
                    theme::key,
                    (changed && !ins.saving).then_some(Message::DiscardRow)
                ),
                space::horizontal(),
            ]
            .spacing(8);
            if ins.row.is_some() && ins.confirm_delete {
                actions = actions
                    .push(small_key("Cancel", theme::key, Some(Message::CancelDelete)))
                    .push(small_key(
                        "Delete row",
                        theme::key_danger_armed,
                        (!ins.saving).then_some(Message::DeleteRow),
                    ));
            } else if ins.row.is_some() {
                actions = actions.push(small_key(
                    "Delete",
                    theme::key_danger,
                    (!ins.saving).then_some(Message::DeleteRow),
                ));
            }
            actions.into()
        };

        container(
            column![
                head,
                scrollable(container(fields).padding(Padding::ZERO.right(12.0)))
                    .style(theme::scroller)
                    .height(Fill),
                column![ins.error.as_deref().map(error_note), footer].spacing(12),
            ]
            .spacing(16),
        )
        .width(340)
        .height(Fill)
        .padding(16)
        .style(theme::bar)
        .into()
    }

    fn status_bar(&self) -> Element<'_, Message> {
        let note: Element<_> = match &self.status {
            Some(Notice::Ok(m)) => text(m.as_str())
                .size(theme::TEXT_XS)
                .style(theme::text_ink2)
                .into(),
            Some(Notice::Error(e)) => text(e.as_str())
                .size(theme::TEXT_XS)
                .style(theme::text_error)
                .into(),
            None => space().into(),
        };
        let figures: Vec<String> = match self.tab {
            Tab::Data => match self.page.ready() {
                Some(p) => vec![count(p.total, "row"), millis(p.result.elapsed)],
                None => vec![],
            },
            Tab::Structure => match self.columns.ready() {
                Some(c) => vec![count(c.len() as u64, "column")],
                None => vec![],
            },
            Tab::Query => match self.query.ready() {
                Some(rs) if !rs.columns.is_empty() => {
                    let mut v = vec![count(rs.rows.len() as u64, "row")];
                    if rs.dropped > 0 {
                        v.push(format!("{} more not shown", thousands(rs.dropped as u64)));
                    }
                    v.push(millis(rs.elapsed));
                    v
                }
                Some(rs) => vec![millis(rs.elapsed)],
                None => vec![],
            },
        };
        let mut right = row![].spacing(20);
        for f in figures {
            right = right.push(
                text(f)
                    .size(theme::TEXT_XS)
                    .style(theme::text_muted)
                    .font(theme::BODY),
            );
        }
        container(row![note, space::horizontal(), right].align_y(Center))
            .padding(Padding::from([0.0, 12.0]))
            .height(28)
            .center_y(28)
            .width(Fill)
            .style(theme::sidebar)
            .into()
    }
}

// ---------- tasks ----------

fn connect(config: Config) -> Task<Message> {
    Task::perform(db::connect(config.clone()), move |r| {
        Message::Connected(config.clone(), r)
    })
}

fn write(db: Db, sql: String, done: String) -> Task<Message> {
    Task::perform(db::execute(db, sql), move |r| {
        Message::RowWritten(match r {
            Ok(rs) if rs.affected == Some(0) => {
                Err("No row matched. It may have changed since this page loaded.".into())
            }
            Ok(_) => Ok(done.clone()),
            Err(e) => Err(e),
        })
    })
}

/// Expand `~/` in file paths and trim stray whitespace.
fn normalized(config: &Config) -> Config {
    let mut c = config.clone();
    for path in [&mut c.path, &mut c.ca_file] {
        *path = path.trim().to_string();
        if let (Some(rest), Some(home)) = (path.strip_prefix("~/"), dirs_next::home_dir()) {
            *path = home.join(rest).to_string_lossy().into_owned();
        }
    }
    c
}

fn file_stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Lead a database error with a plain sentence when the cause is a common one;
/// the driver's own words follow on the next line.
fn explain(error: &str) -> String {
    let lower = error.to_lowercase();
    let plain = if lower.contains("foreign key") {
        "Other rows still point at this one, so the database kept it."
    } else if lower.contains("unique") || lower.contains("duplicate key") {
        "Another row already has this value in a unique column."
    } else if lower.contains("not null") || lower.contains("null value") {
        "A required column is empty."
    } else if lower.contains("invalid input syntax") || lower.contains("datatype mismatch") {
        "A value does not fit its column's type."
    } else {
        return error.to_string();
    };
    format!("{plain}\n{error}")
}

fn count(n: u64, noun: &str) -> String {
    match n {
        1 => format!("1 {noun}"),
        n => format!("{} {noun}s", thousands(n)),
    }
}

fn millis(d: std::time::Duration) -> String {
    let ms = d.as_secs_f64() * 1000.0;
    if ms < 10.0 {
        format!("{ms:.1} ms")
    } else {
        format!("{} ms", ms.round())
    }
}

// ---------- small views ----------

/// The app icon, decoded once.
static ICON: std::sync::LazyLock<svg::Handle> =
    std::sync::LazyLock::new(|| svg::Handle::from_memory(theme::ICON_SVG));

fn top_bar<'a>(
    left: impl Into<Element<'a, Message>>,
    right: Option<Element<'a, Message>>,
    appearance: Appearance,
) -> Element<'a, Message> {
    let modes = container(
        Row::with_children(Appearance::ALL.map(|mode| {
            button(
                text(mode.label())
                    .font(theme::BODY_MEDIUM)
                    .size(theme::TEXT_XS),
            )
            .padding(Padding::from([3.0, 9.0]))
            .style(theme::tray_key(mode == appearance))
            .on_press(Message::SetAppearance(mode))
            .into()
        }))
        .spacing(2),
    )
    .padding(2)
    .style(theme::tray);

    container(
        row![
            row![
                svg(ICON.clone()).width(22).height(22),
                text("sqlsea").font(theme::DISPLAY).size(theme::TEXT_LG),
            ]
            .spacing(8)
            .align_y(Center),
            left.into(),
            space::horizontal(),
            modes,
        ]
        .push(right)
        .spacing(12)
        .align_y(Center),
    )
    .padding(Padding::from([0.0, 16.0]))
    .height(48)
    .center_y(48)
    .width(Fill)
    .style(theme::bar)
    .into()
}

fn hrule<'a>() -> Element<'a, Message> {
    container(space().height(1))
        .width(Fill)
        .style(theme::rule_h)
        .into()
}

fn vrule<'a>() -> Element<'a, Message> {
    container(space().width(1))
        .height(Fill)
        .style(theme::rule_h)
        .into()
}

fn vrule_short<'a>() -> Element<'a, Message> {
    container(space().width(1))
        .height(18)
        .style(theme::rule_h)
        .into()
}

fn small_key<'a>(
    label: &'a str,
    style: fn(&iced::Theme, button::Status) -> button::Style,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    button(
        container(text(label).font(theme::BODY_MEDIUM).size(theme::TEXT_SM)).center_y(Length::Fill),
    )
    .height(theme::CONTROL_H)
    .padding(Padding::from([0.0, 12.0]))
    .style(style)
    .on_press_maybe(on_press)
    .into()
}

fn kind_key<'a>(kind: Kind, chosen: Kind) -> Element<'a, Message> {
    button(
        text(kind.label())
            .font(theme::BODY_MEDIUM)
            .size(theme::TEXT_SM),
    )
    .padding(Padding::from([6.0, 14.0]))
    .style(theme::tray_key(kind == chosen))
    .on_press(Message::FormKind(kind))
    .into()
}

fn input<'a>(
    placeholder: &'a str,
    value: &'a str,
    on_input: fn(String) -> Message,
) -> text_input::TextInput<'a, Message> {
    text_input(placeholder, value)
        .on_input(on_input)
        .on_submit(Message::Connect)
        .padding(Padding::from([7.0, 10.0]))
        .size(theme::TEXT_MD)
        .style(theme::field)
}

fn labelled<'a>(label: &'a str, field: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![
        text(label)
            .font(theme::LABEL)
            .size(theme::TEXT_SM)
            .style(theme::text_ink2),
        field.into()
    ]
    .spacing(6)
    .width(Fill)
    .into()
}

fn muted<'a>(s: impl text::IntoFragment<'a>) -> Element<'a, Message> {
    text(s).size(theme::TEXT_SM).style(theme::text_muted).into()
}

/// An error: a plain first line when there is one, the database's words in mono.
fn error_note<'a>(message: &str) -> Element<'a, Message> {
    let body: Element<_> = match message.split_once('\n') {
        Some((plain, raw)) => column![
            text(plain.to_string())
                .font(theme::BODY_MEDIUM)
                .size(theme::TEXT_SM),
            text(raw.to_string()).font(theme::MONO).size(theme::TEXT_XS),
        ]
        .spacing(4)
        .into(),
        None => text(message.to_string())
            .font(theme::MONO)
            .size(theme::TEXT_SM)
            .into(),
    };
    container(body)
        .padding(Padding::from([10.0, 12.0]))
        .width(Fill)
        .style(theme::error_note)
        .into()
}

fn empty_state<'a>(
    message: impl text::IntoFragment<'a>,
    action: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut col = column![text(message).size(theme::TEXT_MD).style(theme::text_muted)]
        .spacing(14)
        .align_x(Center)
        .max_width(420);
    if let Some(action) = action {
        col = col.push(action);
    }
    container(col).center(Fill).into()
}

fn empty_state_error<'a>(e: &str) -> Element<'a, Message> {
    container(
        column![
            error_note(e),
            small_key("Try again", theme::key, Some(Message::Reload))
        ]
        .spacing(12)
        .max_width(560),
    )
    .center(Fill)
    .padding(24)
    .into()
}

// ---------- screenshots for design review (cargo run --features shot) ----------

/// `SQLSEA_SCRIPT="table:users;row:2;shot:/tmp/a.png;quit"` drives the app
/// one step a second and captures the window.
#[cfg(feature = "shot")]
fn script() -> Task<Message> {
    let Ok(script) = std::env::var("SQLSEA_SCRIPT") else {
        return Task::none();
    };
    let steps: Vec<String> = script.split(';').map(str::to_string).collect();
    Task::batch(steps.into_iter().enumerate().map(|(n, step)| {
        let at = std::time::Duration::from_millis(3000 + 1200 * n as u64);
        let (verb, arg) = step.split_once(':').unwrap_or((step.as_str(), ""));
        let message = match verb {
            "table" => Some(Message::OpenTableNamed(arg.to_string())),
            "row" => arg.parse().ok().map(Message::SelectRow),
            "add" => Some(Message::AddRow),
            "tab" => Some(Message::SwitchTab(match arg {
                "structure" => Tab::Structure,
                "query" => Tab::Query,
                _ => Tab::Data,
            })),
            "sql" => Some(Message::SetSql(arg.replace("\\n", "\n"))),
            "run" => Some(Message::Run),
            "pick" => arg.parse().ok().map(Message::PickSaved),
            "mode" => Some(Message::SetAppearance(match arg {
                "dark" => Appearance::Dark,
                "light" => Appearance::Light,
                _ => Appearance::Auto,
            })),
            "forget" => Some(Message::DeleteSaved),
            "save" => Some(Message::SaveRow),
            "delete" => Some(Message::DeleteRow),
            "edit" => arg
                .split_once('=')
                .and_then(|(i, v)| Some(Message::FieldInput(i.parse().ok()?, v.to_string()))),
            "shot" => Some(Message::Shoot(arg.to_string())),
            "quit" => None,
            _ => return Task::none(),
        };
        Task::perform(tokio::time::sleep(at), move |_| message.clone()).then(|m| match m {
            Some(m) => Task::done(m),
            None => iced::exit(),
        })
    }))
}

#[cfg(feature = "shot")]
fn save_png(path: &str, shot: &iced::window::Screenshot) {
    let file = std::fs::File::create(path).expect("create screenshot");
    let mut encoder = png::Encoder::new(file, shot.size.width, shot.size.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(&shot.rgba).expect("png data");
}
