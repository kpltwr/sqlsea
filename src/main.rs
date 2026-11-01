// Release builds on Windows open no console window behind the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod chrome;
mod complete;
mod db;
mod diagram;
mod dsn;
mod export;
mod fit;
mod grid;
mod guard;
mod mark;
mod query;
mod sql;
mod store;
mod theme;
mod tls;
mod tunnel;

use iced::keyboard::{self, Key, key::Named};
use iced::widget::{
    Column, Row, button, checkbox, column, container, image, operation, pick_list, responsive, row,
    scrollable, space, text, text_editor, text_input,
};
use iced::{Center, Element, Fill, Length, Padding, Subscription, Task};

use db::{Config, Db, Kind, Page, ResultSet, Table, Tls};
use fit::fit;
use store::Appearance;

const PAGE: u64 = 200;
/// The name of the shortcut key on macOS and on other systems.
const RUN_KEY: &str = if cfg!(target_os = "macos") {
    "Cmd"
} else {
    "Ctrl"
};
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
            size: window_size(),
            min_size: Some(iced::Size::new(760.0, 520.0)),
            icon: window_icon(),
            // On Linux, the app draws its own title bar. See `chrome`.
            decorations: !chrome::CUSTOM,
            #[cfg(target_os = "linux")]
            platform_specific: iced::window::settings::PlatformSpecific {
                // Same as in packaging/linux/sqlsea.desktop, so that launchers show the icon.
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

/// 1280 by 800. Screenshot builds read `SQLSEA_SIZE=WxH` to test other sizes.
fn window_size() -> iced::Size {
    #[cfg(feature = "shot")]
    if let Some((w, h)) = std::env::var("SQLSEA_SIZE")
        .ok()
        .and_then(|s| s.split_once('x').map(|(w, h)| (w.parse(), h.parse())))
        .and_then(|(w, h)| Some((w.ok()?, h.ok()?)))
    {
        return iced::Size::new(w, h);
    }
    iced::Size::new(1280.0, 800.0)
}

fn window_icon() -> Option<iced::window::Icon> {
    let (rgba, width, height) = decode_png(include_bytes!("../brand/png/icon-256.png"))?;
    iced::window::icon::from_rgba(rgba, width, height).ok()
}

/// RGBA pixels, width and height of a PNG image.
fn decode_png(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .ok()?;
    let mut rgba = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut rgba).ok()?;
    rgba.truncate(info.buffer_size());
    Some((rgba, info.width, info.height))
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
    Diagram,
}

#[derive(Debug, Clone)]
enum Notice {
    Ok(String),
    Error(String),
}

struct App {
    saved: Vec<Config>,
    form: Config,
    /// The index in `saved` of the connection in the form, if it is saved.
    editing: Option<usize>,
    busy: bool,
    notice: Option<Notice>,
    /// A connection URL that the user pasted, before it fills in the form.
    url: String,
    session: Option<Session>,
    appearance: Appearance,
    /// The theme of the desktop, for `Appearance::Auto`.
    system_dark: bool,
}

struct Session {
    /// Keeps the SSH tunnel open while the session is open.
    _tunnel: Option<std::sync::Arc<tunnel::Tunnel>>,
    /// True if the PostgreSQL connection is encrypted. `None` for SQLite.
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
    /// Goes up on each table change or reload, so that late answers are ignored.
    generation: u64,
    queries: query::Queries,
    status: Option<Notice>,
    /// The text in the row filter box.
    row_filter: String,
    /// The filter that the current page uses.
    applied_filter: Option<String>,
    /// The column names of each table, for search and suggestions.
    column_index: ColumnIndex,
    /// Grid changes that are not saved yet.
    changes: grid::Changes,
    /// The cell being edited in the grid: row, column and text.
    cell_edit: Option<(usize, usize, String)>,
    show_review: bool,
    saving_changes: bool,
    changes_error: Option<String>,
    structure: Load<db::Structure>,
    /// Goes up with each structure load, so that late answers are ignored.
    structure_run: u64,
    structure_view: StructureView,
    structure_sql: text_editor::Content,
    /// For each grid, its scroll position and visible height.
    scrolled: std::collections::HashMap<&'static str, (f32, f32)>,
    diagram: Load<diagram::Diagram>,
    /// Goes up with each diagram load, so that late answers are ignored.
    diagram_run: u64,
}

/// The column names of each table, by schema and table name.
type ColumnIndex = std::collections::HashMap<(Option<String>, String), Vec<String>>;

/// The parts of the Structure tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StructureView {
    Columns,
    Indexes,
    ForeignKeys,
    ReferencedBy,
    Sql,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Move {
    Up,
    Down,
    PageUp,
    PageDown,
    First,
    Last,
}

/// The columns of a table, and the Structure tab rows, made once at load.
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
    /// A value with line breaks uses a multi-line editor.
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
    FormTimeout(String),
    FormPath(String),
    BrowseDatabase,
    BrowseCaFile,
    /// The field to fill, and the file that the user chose, if any.
    FilePicked(FileField, Option<String>),
    Test,
    Tested(Result<Option<bool>, String>),
    Connect,
    Connected(Box<Config>, Result<db::Connected, String>),
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
    /// Selects a row. An open row panel follows the selection.
    SelectRow(usize),
    /// Opens a row in the row panel.
    OpenRow(usize),
    EditCell(usize, usize),
    CellInput(String),
    CommitCell,
    /// Sets the edited cell to NULL.
    CellNull,
    ToggleDeleteRow,
    ToggleReview,
    SaveChanges,
    DiscardChanges,
    ChangesSaved(Result<usize, String>),
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
    /// The query tab id, the run number and the result.
    QueryDone(u64, u64, Result<ResultSet, String>),
    SelectQueryRow(usize),
    Cancel,
    Cancelled(Result<(), String>),
    NewQuery,
    CloseQuery(u64),
    SelectQuery(u64),
    ToggleHistory,
    HistoryFilter(String),
    UseHistory(usize),
    /// Updates the time shown for running queries.
    Tick,
    ExportQuery,
    ExportTable,
    /// The saved file and its row count, or nothing if the user closed the dialog.
    Exported(Result<Option<(String, usize, Option<String>)>, String>),
    CopyRow,
    GridScrolled(grid::Scrolled),
    /// Moves the row selection with the keyboard.
    MoveSelection(Move),
    RowFilter(String),
    ApplyFilter,
    ClearFilter,
    /// Takes suggestion `n` in the SQL editor.
    Complete(usize),
    SwitchStructure(StructureView),
    StructureLoaded(u64, Result<db::Structure, String>),
    DiagramLoaded(u64, Result<db::Relations, String>),
    DiagramSchema(String),
    /// Opens a table from the diagram in the Data tab.
    DiagramOpen(Table),
    /// A table moved in the diagram: the layout, the table and its new place.
    DiagramMoved(u64, usize, iced::Point),
    /// Forgets the places of the tables that were moved by hand.
    DiagramArrange,
    /// Edits are ignored, so the SQL in the Structure tab is read-only.
    StructureSqlAction(text_editor::Action),
    CopyStructureSql,
    /// Runs the statements that the safety check stopped.
    RunAnyway,
    DismissRisks,
    FormReadOnly(bool),
    FormUrl(String),
    FormSsh(bool),
    FormSshHost(String),
    FormSshPort(String),
    FormSshUser(String),
    FormSshAuth(tunnel::SshAuth),
    FormSshKeyFile(String),
    FormSshSecret(String),
    BrowseSshKey,
    ApplyUrl,
    ColumnIndex(Result<ColumnIndex, String>),
    Key(keyboard::Event),
    SetAppearance(Appearance),
    Window(chrome::Action),
    SystemTheme(iced::theme::Mode),
    #[cfg(feature = "shot")]
    Shoot(String),
    #[cfg(feature = "shot")]
    ScrollFormEnd,
    #[cfg(feature = "shot")]
    ScrollQueryGrid(f32),
}

impl App {
    fn boot() -> (Self, Task<Message>) {
        let (saved, keyring_problem) = store::load();
        let mut app = App {
            form: Config::default(),
            editing: None,
            busy: false,
            notice: keyring_problem.map(Notice::Error),
            url: String::new(),
            session: None,
            saved,
            appearance: store::load_settings().appearance,
            system_dark: false,
        };
        let mut task = iced::system::theme().map(Message::SystemTheme);
        // `sqlsea app.db` opens a SQLite file. `sqlsea name` opens a saved connection.
        if let Some(arg) = std::env::args().nth(1) {
            // A file uses the saved connection to the same file, so that it keeps
            // its id, and with it its history and diagram places.
            let same_file = |c: &Config| {
                c.kind == Kind::Sqlite
                    && match (std::fs::canonicalize(&c.path), std::fs::canonicalize(&arg)) {
                        (Ok(a), Ok(b)) => a == b,
                        _ => c.path == arg,
                    }
            };
            if let Some(i) = app
                .saved
                .iter()
                .position(|c| c.name == arg)
                .or_else(|| app.saved.iter().position(same_file))
            {
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
            // Connections that were saved without an id get one now.
            if app.form.id.is_empty() {
                app.form.id = store::new_id();
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
        let running = self
            .session
            .as_ref()
            .is_some_and(|s| s.queries.any_running());
        Subscription::batch([
            keyboard::listen().map(Message::Key),
            iced::system::theme_changes().map(Message::SystemTheme),
            if running {
                iced::time::every(std::time::Duration::from_millis(100)).map(|_| Message::Tick)
            } else {
                Subscription::none()
            },
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
                self.url.clear();
            }
            Message::NewConnection => {
                self.form = Config::default();
                self.editing = None;
                self.notice = None;
                self.url.clear();
            }
            Message::FormUrl(v) => self.url = v,
            Message::FormSsh(v) => self.form.ssh = v,
            Message::FormSshHost(v) => self.form.ssh_host = v,
            Message::FormSshPort(v) => self.form.ssh_port = v,
            Message::FormSshUser(v) => self.form.ssh_user = v,
            Message::FormSshAuth(v) => self.form.ssh_auth = v,
            Message::FormSshKeyFile(v) => self.form.ssh_key_file = v,
            Message::FormSshSecret(v) => self.form.ssh_secret = v,
            Message::BrowseSshKey => {
                let current = if self.form.ssh_key_file.trim().is_empty() {
                    "~/.ssh/id_ed25519"
                } else {
                    &self.form.ssh_key_file
                };
                return pick_file(FileField::SshKey, current);
            }
            Message::ApplyUrl => {
                let mut form = self.form.clone();
                match dsn::apply(&self.url, &mut form) {
                    Ok(()) => {
                        self.form = form;
                        // The URL can hold the password, so it does not stay on screen.
                        self.url.clear();
                        self.notice =
                            Some(Notice::Ok("The form is filled in from the URL.".into()));
                    }
                    Err(e) => self.notice = Some(Notice::Error(e)),
                }
            }
            Message::FormKind(kind) => {
                // A message about one kind of database is not correct for the other kind.
                if self.form.kind != kind {
                    self.notice = None;
                }
                self.form.kind = kind;
            }
            Message::FormName(v) => self.form.name = v,
            Message::FormHost(v) => self.form.host = v,
            Message::FormPort(v) => self.form.port = v,
            Message::FormUser(v) => self.form.user = v,
            Message::FormPassword(v) => self.form.password = v,
            Message::FormDatabase(v) => self.form.database = v,
            Message::FormTls(v) => self.form.tls = v,
            Message::FormCaFile(v) => self.form.ca_file = v,
            Message::FormTimeout(v) => self.form.timeout = v,
            Message::FormReadOnly(v) => self.form.read_only = v,
            Message::FormPath(v) => self.form.path = v,
            Message::BrowseDatabase => {
                return pick_file(FileField::Database, &self.form.path);
            }
            Message::BrowseCaFile => return pick_file(FileField::CaFile, &self.form.ca_file),
            Message::FilePicked(field, Some(path)) => match field {
                FileField::Database => {
                    if self.form.name.trim().is_empty() {
                        self.form.name = file_stem(&path);
                    }
                    self.form.path = path;
                }
                FileField::CaFile => self.form.ca_file = path,
                FileField::SshKey => self.form.ssh_key_file = path,
            },
            Message::FilePicked(_, None) => {}
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
                if let Err(e) = config.timeout_secs() {
                    self.busy = false;
                    self.notice = Some(Notice::Error(e));
                    return Task::none();
                }
                return connect(config);
            }
            Message::Connected(config, result) => {
                let config = *config;
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
                    store::delete_connection_files(&gone.id);
                    self.form = Config::default();
                    self.notice = store::save(&self.saved)
                        .and_then(|()| store::delete_password(&gone.id))
                        .and_then(|()| store::delete_password(&store::ssh_key(&gone.id)))
                        .err()
                        .map(Notice::Error);
                }
            }
            Message::Disconnect if self.session.as_mut().is_some_and(Session::blocked) => {}
            Message::Disconnect => {
                self.session = None;
                self.notice = None;
            }
            Message::Key(event) => return self.on_key(event),
            Message::SetAppearance(appearance) => {
                self.appearance = appearance;
                if let Err(e) = store::save_settings(&store::Settings { appearance }) {
                    self.notice = Some(Notice::Error(format!("The setting was not saved: {e}")));
                }
            }
            Message::Window(action) => return chrome::perform(action),
            Message::SystemTheme(mode) => self.system_dark = mode == iced::theme::Mode::Dark,
            Message::GridScrolled(scrolled) => {
                if let Some(session) = &mut self.session {
                    session
                        .scrolled
                        .insert(scrolled.body, (scrolled.y, scrolled.height));
                }
                return operation::scroll_to(
                    scrolled.head,
                    operation::AbsoluteOffset {
                        x: Some(scrolled.x),
                        y: None,
                    },
                );
            }
            #[cfg(feature = "shot")]
            Message::ScrollQueryGrid(y) => {
                return operation::scroll_to(
                    QUERY_GRID.0,
                    operation::AbsoluteOffset {
                        x: None,
                        y: Some(y),
                    },
                );
            }
            #[cfg(feature = "shot")]
            Message::ScrollFormEnd => {
                return operation::snap_to("connect-form", operation::RelativeOffset::END);
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

    /// Puts a connection that worked at the top of the saved list.
    fn remember(&mut self, config: &Config) {
        if let Some(i) = self.editing.take() {
            self.saved.remove(i);
        }
        // Replace a different connection with the same name, and delete its password.
        for old in self
            .saved
            .iter()
            .filter(|c| c.name == config.name && c.id != config.id)
        {
            let _ = store::delete_password(&old.id);
            let _ = store::delete_password(&store::ssh_key(&old.id));
        }
        self.saved.retain(|c| c.name != config.name);
        self.saved.insert(0, config.clone());
        self.editing = Some(0);
        if let Err(e) = store::save(&self.saved) {
            self.notice = Some(Notice::Error(format!("The connection was not saved: {e}")));
        } else if let Err(e) = store::set_password(&config.id, &config.password).and_then(|()| {
            let ssh_secret = if config.ssh {
                config.ssh_secret.as_str()
            } else {
                ""
            };
            store::set_password(&store::ssh_key(&config.id), ssh_secret)
        }) {
            self.notice = Some(Notice::Error(format!(
                "Connected. The password was not saved, because the system keyring \
                 did not accept it.\n{e}"
            )));
        }
    }

    fn on_key(&mut self, event: keyboard::Event) -> Task<Message> {
        let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
            return Task::none();
        };
        match key.as_ref() {
            Key::Named(Named::Tab)
                if self.session.as_ref().is_some_and(|s| s.cell_edit.is_some()) =>
            {
                self.session
                    .as_mut()
                    .map_or(Task::none(), |s| s.on_key(key, modifiers))
            }
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
        let content = match &self.session {
            Some(session) => session.view(self.appearance),
            None => self.connect_view(),
        };
        chrome::frame(content, Message::Window)
    }

    fn connect_view(&self) -> Element<'_, Message> {
        let list: Element<_> = if self.saved.is_empty() {
            text("No saved connections yet. Fill in the form to add your first one.")
                .size(theme::TEXT_MD)
                .style(theme::text_muted)
                .wrapping(text::Wrapping::WordOrGlyph)
                .into()
        } else {
            Column::with_children(self.saved.iter().enumerate().map(|(i, c)| {
                button(
                    column![
                        fit(c.name.as_str())
                            .font(theme::BODY_MEDIUM)
                            .size(theme::TEXT_MD),
                        fit(c.target())
                            .font(theme::MONO)
                            .size(theme::TEXT_XS)
                            .style(theme::text_muted),
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
        .spacing(16);

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

        let mut fields = column![].spacing(14);
        if f.kind == Kind::Postgres {
            fields = fields.push(labelled(
                "Connection URL",
                row![
                    text_input(
                        "Paste a URL, for example postgres://user@host/db",
                        &self.url
                    )
                    .on_input(Message::FormUrl)
                    .on_submit(Message::ApplyUrl)
                    .font(theme::MONO)
                    .padding(Padding::from([7.0, 10.0]))
                    .size(theme::TEXT_MD)
                    .style(theme::field)
                    .width(Fill),
                    small_key(
                        "Fill in",
                        theme::key,
                        (!self.url.trim().is_empty()).then_some(Message::ApplyUrl),
                    ),
                ]
                .spacing(8)
                .align_y(Center),
            ));
        }
        fields = fields.push(labelled(
            "Name",
            input("My database", &f.name, Message::FormName),
        ));
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
                            input("Kept in your keyring", &f.password, Message::FormPassword)
                                .secure(true)
                        ),
                    ]
                    .spacing(12),
                )
                .push(
                    row![
                        labelled(
                            "Database",
                            input("Same as the user", &f.database, Message::FormDatabase),
                        ),
                        container(labelled(
                            "Time limit, s",
                            input("None", &f.timeout, Message::FormTimeout)
                        ))
                        .width(110),
                    ]
                    .spacing(12),
                )
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
                        with_browse(
                            input(
                                "Optional. A PEM file from your provider",
                                &f.ca_file,
                                Message::FormCaFile,
                            )
                            .font(theme::MONO),
                            Message::BrowseCaFile,
                        ),
                    )
                })),
            Kind::Sqlite => fields
                .push(labelled(
                    "Database file",
                    with_browse(
                        input("~/data/app.db", &f.path, Message::FormPath).font(theme::MONO),
                        Message::BrowseDatabase,
                    ),
                ))
                .push(
                    container(labelled(
                        "Time limit, s",
                        input("None", &f.timeout, Message::FormTimeout),
                    ))
                    .width(110),
                ),
        };

        let fields = if f.kind == Kind::Postgres {
            fields.push(ssh_fields(f))
        } else {
            fields
        };
        let fields = fields.push(
            column![
                checkbox(f.read_only)
                    .label("Read-only")
                    .on_toggle(Message::FormReadOnly)
                    .font(theme::BODY_MEDIUM)
                    .text_size(theme::TEXT_SM)
                    .size(16)
                    .spacing(8)
                    .style(theme::check_box),
                muted("The database refuses all changes on this connection."),
            ]
            .spacing(4),
        );

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
        .width(Fill)
        .style(theme::panel);
        // If the window is short, the form scrolls.
        let form = scrollable(form)
            .id("connect-form")
            .style(theme::scroller)
            .width(Fill);

        // The list gets two parts of the width and the form gets three, up to a limit.
        let columns = row![
            container(saved)
                .width(Length::FillPortion(2))
                .max_width(320),
            container(form).width(Length::FillPortion(3)).max_width(480),
        ]
        .spacing(32);
        let body = container(container(columns).max_width(832))
            .center_x(Fill)
            .padding(Padding::from([32.0, 24.0]))
            .height(Fill);

        column![top_bar(row![], None, self.appearance), hrule(), body].into()
    }
}

impl Session {
    fn new(config: Config, connected: db::Connected) -> Self {
        let history = store::load_history(&config.id);
        Session {
            _tunnel: connected.tunnel,
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
            queries: query::Queries::new(history),
            status: None,
            row_filter: String::new(),
            applied_filter: None,
            scrolled: std::collections::HashMap::new(),
            column_index: ColumnIndex::new(),
            changes: grid::Changes::default(),
            cell_edit: None,
            show_review: false,
            saving_changes: false,
            changes_error: None,
            structure: Load::Idle,
            structure_run: 0,
            structure_view: StructureView::Columns,
            structure_sql: text_editor::Content::new(),
            diagram: Load::Idle,
            diagram_run: 0,
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
        self.row_filter.clear();
        self.applied_filter = None;
        self.selected = None;
        self.inspector = None;
        if self.tab == Tab::Query {
            self.tab = Tab::Data;
        }
        self.load_columns()
    }

    /// True if there are grid changes, after it tells the user to save or discard them.
    fn blocked(&mut self) -> bool {
        self.commit_cell();
        let n = self.changes.edited.len() + self.changes.deleted.len();
        if n == 0 {
            return false;
        }
        self.status = Some(Notice::Error(format!(
            "Save or discard the {} in the grid first.",
            count(n as u64, "change")
        )));
        true
    }

    fn row_has_changes(&self, r: usize) -> bool {
        self.changes.deleted.contains(&r) || self.changes.edited.keys().any(|(row, _)| *row == r)
    }

    /// Starts to edit a cell in the grid.
    fn edit_cell(&mut self, r: usize, c: usize) -> Task<Message> {
        self.commit_cell();
        if !self.editable() {
            self.status = Some(Notice::Error(
                if self.config.read_only {
                    "This connection is read-only."
                } else if self.current.as_ref().is_some_and(|t| t.is_view) {
                    "Views are read-only here."
                } else {
                    "This table has no primary key, so its rows are read-only here."
                }
                .into(),
            ));
            return Task::none();
        }
        if self.changes.deleted.contains(&r) {
            return Task::none();
        }
        let Some(original) = self
            .page
            .ready()
            .and_then(|p| p.result.rows.get(r))
            .and_then(|row| row.get(c))
            .cloned()
        else {
            return Task::none();
        };
        let current = self
            .changes
            .edited
            .get(&(r, c))
            .cloned()
            .unwrap_or(original);
        // A value with line breaks needs the multi-line editor of the row panel.
        if current.as_deref().is_some_and(|v| v.contains('\n')) {
            self.status = Some(Notice::Ok(
                "Values with line breaks open in the row panel.".into(),
            ));
            return self.update(Message::OpenRow(r));
        }
        self.inspector = None;
        self.selected = Some(r);
        self.cell_edit = Some((r, c, current.unwrap_or_default()));
        operation::focus("cell-edit")
    }

    /// Keeps the text of the edited cell as a change, if it differs.
    fn commit_cell(&mut self) {
        let Some((r, c, value)) = self.cell_edit.take() else {
            return;
        };
        let original = self
            .page
            .ready()
            .and_then(|p| p.result.rows.get(r))
            .and_then(|row| row.get(c))
            .cloned()
            .flatten();
        // An empty cell that was NULL stays NULL.
        if original.as_deref() == Some(value.as_str()) || (original.is_none() && value.is_empty()) {
            self.changes.edited.remove(&(r, c));
        } else {
            self.changes.edited.insert((r, c), Some(value));
        }
    }

    /// The statements that save the grid changes: updates, then deletes.
    fn change_sql(&self) -> Vec<String> {
        let (Some(table), Some(columns), Some(page)) = (
            self.current.as_ref(),
            self.columns.ready(),
            self.page.ready(),
        ) else {
            return Vec::new();
        };
        let mut rows: std::collections::BTreeMap<usize, Vec<(usize, Option<String>)>> =
            std::collections::BTreeMap::new();
        for ((r, c), value) in &self.changes.edited {
            rows.entry(*r).or_default().push((*c, value.clone()));
        }
        let mut out = Vec::new();
        for (r, changes) in rows {
            if self.changes.deleted.contains(&r) {
                continue;
            }
            if let Some(sql) = db::update_sql(table, columns, &page.result.rows[r], &changes) {
                out.push(sql);
            }
        }
        for r in &self.changes.deleted {
            if let Some(sql) = db::delete_sql(table, columns, &page.result.rows[*r]) {
                out.push(sql);
            }
        }
        out
    }

    /// Saves the places of the tables in the diagram that were moved by hand.
    fn save_places(&mut self) -> Task<Message> {
        if let Load::Ready(d) = &self.diagram
            && let Err(e) = store::save_places(&self.config.id, d.places())
        {
            self.status = Some(Notice::Error(format!(
                "The places of the tables were not saved: {e}"
            )));
        }
        Task::none()
    }

    /// Loads the tables and keys for the diagram, while the Diagram tab shows and they are not loaded.
    fn load_diagram(&mut self) -> Task<Message> {
        if self.tab != Tab::Diagram || !matches!(self.diagram, Load::Idle) {
            return Task::none();
        }
        self.diagram = Load::Loading;
        self.diagram_run += 1;
        let run = self.diagram_run;
        Task::perform(db::relations(self.db.clone()), move |r| {
            Message::DiagramLoaded(run, r)
        })
    }

    /// Loads indexes, keys and SQL, while the Structure tab shows and they are not loaded.
    fn load_structure(&mut self) -> Task<Message> {
        if self.tab != Tab::Structure || !matches!(self.structure, Load::Idle) {
            return Task::none();
        }
        let (Some(table), Some(columns)) = (self.current.clone(), self.columns.ready()) else {
            return Task::none();
        };
        self.structure = Load::Loading;
        self.structure_run += 1;
        let run = self.structure_run;
        Task::perform(
            db::structure(self.db.clone(), table, columns.list.clone()),
            move |r| Message::StructureLoaded(run, r),
        )
    }

    fn load_columns(&mut self) -> Task<Message> {
        let Some(table) = self.current.clone() else {
            return Task::none();
        };
        self.generation += 1;
        let generation = self.generation;
        self.columns = Load::Loading;
        self.page = Load::Loading;
        self.structure = Load::Idle;
        Task::perform(db::columns(self.db.clone(), table), move |r| {
            Message::ColumnsLoaded(generation, r)
        })
    }

    /// The row order: the sort column if there is one, otherwise the primary key.
    /// A fixed order keeps the same rows on each page.
    fn order(&self) -> Option<(String, bool)> {
        let columns = self.columns.ready()?;
        match self.sort {
            Some((i, asc)) => columns.get(i).map(|c| (c.name.clone(), asc)),
            None => columns
                .iter()
                .find(|c| c.primary_key)
                .map(|c| (c.name.clone(), true)),
        }
    }

    fn load_page(&mut self) -> Task<Message> {
        let (Some(table), Some(_)) = (self.current.clone(), self.columns.ready()) else {
            return Task::none();
        };
        let order = self.order();
        self.generation += 1;
        let generation = self.generation;
        self.page = Load::Loading;
        self.selected = None;
        self.inspector = None;
        let db = self.db.clone();
        let offset = self.offset;
        let filter = self.applied_filter.clone();
        // The snap to the first row is chained below. Here only the stored
        // position changes, so that the first frame builds the first rows.
        if let Some((y, _)) = self.scrolled.get_mut(DATA_GRID.0) {
            *y = 0.0;
        }
        Task::perform(db::page(db, table, filter, order, offset, PAGE), move |r| {
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
        has_key && !self.config.read_only && self.current.as_ref().is_some_and(|t| !t.is_view)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // Grid changes that are not saved block anything that reloads the rows.
            Message::PrevPage
            | Message::NextPage
            | Message::Sort(_)
            | Message::ApplyFilter
            | Message::ClearFilter
            | Message::OpenTable(_)
            | Message::Refresh
            | Message::Reload
                if self.blocked() =>
            {
                return Task::none();
            }
            Message::Refresh => {
                self.diagram = Load::Idle;
                let tables = Task::batch([self.load_tables(), self.load_diagram()]);
                return match self.current {
                    Some(_) => Task::batch([tables, self.load_columns()]),
                    None => tables,
                };
            }
            Message::TablesLoaded(result) => match result {
                Ok(tables) => {
                    // Keep the open table if it still exists. If not, open the first table.
                    let keep = self.current.as_ref().is_some_and(|t| tables.contains(t));
                    let first = tables.first().cloned();
                    self.tables = Load::Ready(tables);
                    // Search by column name needs every column, which can take a moment.
                    let index =
                        Task::perform(db::column_index(self.db.clone()), Message::ColumnIndex);
                    if !keep {
                        self.current = None;
                        if let Some(first) = first.filter(|_| self.tab != Tab::Query) {
                            return Task::batch([index, self.open(first)]);
                        }
                    }
                    return index;
                }
                Err(e) => self.tables = Load::Failed(e),
            },
            // Without the index, search still finds tables by name.
            Message::ColumnIndex(result) => self.column_index = result.unwrap_or_default(),
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
                return Task::batch([self.load_structure(), self.load_diagram()]);
            }
            Message::DiagramLoaded(run, result) => {
                if run == self.diagram_run {
                    let prefer = self.current.as_ref().and_then(|t| t.schema.clone());
                    self.diagram = match result {
                        Ok(r) => {
                            let places = store::load_places(&self.config.id);
                            Load::Ready(diagram::Diagram::new(r, prefer.as_deref(), places))
                        }
                        Err(e) => Load::Failed(e),
                    };
                }
            }
            Message::DiagramSchema(schema) => {
                if let Load::Ready(d) = &mut self.diagram {
                    d.set_schema(schema);
                }
            }
            Message::DiagramMoved(layout, node, to) => {
                if let Load::Ready(d) = &mut self.diagram {
                    d.move_table(layout, node, to);
                    return self.save_places();
                }
            }
            Message::DiagramArrange => {
                if let Load::Ready(d) = &mut self.diagram {
                    d.arrange_again();
                    return self.save_places();
                }
            }
            Message::DiagramOpen(table) => {
                self.tab = Tab::Data;
                return self.open(table);
            }
            Message::SwitchStructure(view) => {
                self.structure_view = view;
                return self.scroll_to_top(STRUCTURE_GRID.0);
            }
            Message::StructureLoaded(run, result) => {
                if run == self.structure_run {
                    if let Ok(structure) = &result {
                        self.structure_sql = text_editor::Content::with_text(&structure.sql);
                    }
                    self.structure = match result {
                        Ok(s) => Load::Ready(s),
                        Err(e) => Load::Failed(e),
                    };
                }
            }
            Message::StructureSqlAction(action) => {
                if !action.is_edit() {
                    self.structure_sql.perform(action);
                }
            }
            Message::CopyStructureSql => {
                if let Some(structure) = self.structure.ready() {
                    self.status = Some(Notice::Ok("Copied the SQL.".into()));
                    return iced::clipboard::write(structure.sql.clone());
                }
            }
            Message::Complete(index) => {
                let tables = self.tables.ready().map(Vec::as_slice).unwrap_or(&[]);
                let catalog = complete::Catalog {
                    tables,
                    columns: &self.column_index,
                };
                self.queries
                    .tab_mut_for_completion()
                    .accept(index, &catalog);
            }
            Message::ColumnsLoaded(generation, result) => {
                if generation != self.generation {
                    return Task::none();
                }
                match result {
                    Ok(columns) => {
                        self.columns = Load::Ready(Columns::new(columns));
                        let page = self.load_page();
                        return Task::batch([page, self.load_structure()]);
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
                self.commit_cell();
                self.selected = Some(r);
                if self.inspector.is_some() {
                    return self.update(Message::OpenRow(r));
                }
            }
            Message::OpenRow(r) => {
                self.commit_cell();
                if self.row_has_changes(r) {
                    self.status = Some(Notice::Error(
                        "Save or discard the changes to this row first.".into(),
                    ));
                    return Task::none();
                }
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
            Message::EditCell(r, c) => return self.edit_cell(r, c),
            Message::CellInput(text) => {
                if let Some((_, _, value)) = &mut self.cell_edit {
                    *value = text;
                }
            }
            Message::CommitCell => self.commit_cell(),
            Message::CellNull => {
                let Some((r, c, _)) = self.cell_edit else {
                    return Task::none();
                };
                let Some(column) = self.columns.ready().and_then(|cols| cols.get(c)) else {
                    return Task::none();
                };
                if !column.nullable {
                    self.status = Some(Notice::Error(format!(
                        "The column {} cannot be NULL.",
                        column.name
                    )));
                    return Task::none();
                }
                self.cell_edit = None;
                let original = self
                    .page
                    .ready()
                    .and_then(|p| p.result.rows.get(r))
                    .and_then(|row| row.get(c))
                    .cloned()
                    .flatten();
                if original.is_none() {
                    self.changes.edited.remove(&(r, c));
                } else {
                    self.changes.edited.insert((r, c), None);
                }
            }
            Message::ToggleDeleteRow => {
                if let Some(r) = self.selected
                    && self.editable()
                    && self.inspector.is_none()
                {
                    self.commit_cell();
                    if !self.changes.deleted.remove(&r) {
                        self.changes.deleted.insert(r);
                    }
                }
            }
            Message::ToggleReview => self.show_review = !self.show_review,
            Message::DiscardChanges => {
                self.changes = grid::Changes::default();
                self.cell_edit = None;
                self.show_review = false;
                self.changes_error = None;
            }
            Message::SaveChanges => {
                self.commit_cell();
                let statements = self.change_sql();
                if statements.is_empty() || self.saving_changes {
                    return Task::none();
                }
                self.saving_changes = true;
                self.changes_error = None;
                return Task::perform(
                    db::apply(self.db.clone(), statements),
                    Message::ChangesSaved,
                );
            }
            Message::ChangesSaved(result) => {
                self.saving_changes = false;
                match result {
                    Ok(n) => {
                        self.changes = grid::Changes::default();
                        self.show_review = false;
                        self.status =
                            Some(Notice::Ok(format!("Saved {}.", count(n as u64, "change"))));
                        return self.load_page();
                    }
                    Err(e) => self.changes_error = Some(explain(&e)),
                }
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
            Message::Edit(action) => {
                let tab = self.queries.active_mut();
                if action.is_edit() {
                    tab.risks = None;
                }
                tab.editor.perform(action);
            }
            Message::RunAnyway => return self.run_checked(true),
            Message::DismissRisks => self.queries.active_mut().risks = None,
            Message::RowFilter(text) => self.row_filter = text,
            Message::ApplyFilter => {
                let filter = self.row_filter.trim();
                self.applied_filter = (!filter.is_empty()).then(|| filter.to_string());
                self.offset = 0;
                return self.load_page();
            }
            Message::ClearFilter => {
                self.row_filter.clear();
                self.applied_filter = None;
                self.offset = 0;
                return self.load_page();
            }
            Message::MoveSelection(step) => return self.move_selection(step),
            #[cfg(feature = "shot")]
            Message::SetSql(sql) => self.queries.active_mut().set_sql(sql),
            Message::Run => return self.run(),
            Message::QueryDone(tab, run, result) => {
                let changed_schema = matches!(&result, Ok(rs) if rs.columns.is_empty());
                let Some(entry) = self
                    .queries
                    .find_mut(tab)
                    .and_then(|t| t.finish(run, result))
                else {
                    return Task::none();
                };
                store::record(&mut self.queries.history, entry);
                if let Err(e) = store::save_history(&self.config.id, &self.queries.history) {
                    self.status = Some(Notice::Error(format!("The history was not saved: {e}")));
                }
                // A new result starts at its first row.
                let top = self.scroll_to_top(QUERY_GRID.0);
                // A statement without rows can add or remove a table, so reload the list.
                if changed_schema {
                    return Task::batch([top, self.load_tables()]);
                }
                return top;
            }
            Message::SelectQueryRow(r) => self.queries.active_mut().selected = Some(r),
            Message::Cancel => {
                let tab = self.queries.active_mut();
                if tab.running() && !tab.cancelling {
                    tab.cancelling = true;
                    return Task::perform(
                        db::cancel(self.db.clone(), self.config.clone()),
                        Message::Cancelled,
                    );
                }
            }
            Message::Cancelled(Err(e)) => {
                self.queries.active_mut().cancelling = false;
                self.status = Some(Notice::Error(format!("The query did not stop: {e}")));
            }
            Message::NewQuery => {
                self.queries.add();
                self.tab = Tab::Query;
                // The new tab is the last one. Scroll the tab strip to show it.
                return Task::batch([
                    operation::focus("sql"),
                    operation::snap_to("query-tabs", operation::RelativeOffset::END),
                ]);
            }
            Message::CloseQuery(id) => self.queries.close(id),
            Message::SelectQuery(id) => {
                self.queries.select(id);
                let top = self.scroll_to_top(QUERY_GRID.0);
                return Task::batch([top, operation::focus("sql")]);
            }
            Message::ToggleHistory => self.queries.show_history = !self.queries.show_history,
            Message::HistoryFilter(f) => self.queries.history_filter = f,
            Message::UseHistory(i) => {
                if let Some(entry) = self.queries.history.get(i) {
                    let sql = entry.sql.clone();
                    self.queries.active_mut().set_sql(sql);
                    return operation::focus("sql");
                }
            }
            Message::ExportQuery => {
                let tab = self.queries.active();
                if let Load::Ready(rs) = &tab.result {
                    let name = format!("query-{}", tab.id);
                    if rs.dropped == 0 {
                        let rs = rs.clone();
                        return export(name, None, async move { Ok(rs) });
                    }
                    // The result kept only some rows. To export all of them, run the
                    // query again, but only if it only reads.
                    if guard::reads_only(&tab.sql) {
                        return export(
                            name,
                            None,
                            db::execute_all_limited(
                                self.db.clone(),
                                self.config.clone(),
                                tab.sql.clone(),
                            ),
                        );
                    }
                    let note = format!(
                        "The query changes data, so sqlsea did not run it again for the other {}.",
                        count(rs.dropped as u64, "row")
                    );
                    let rs = rs.clone();
                    return export(name, Some(note), async move { Ok(rs) });
                }
            }
            Message::ExportTable => {
                if let Some(table) = self.current.clone() {
                    let order = self.order();
                    let sql = format!(
                        "select * from {}{}",
                        table.qualified(),
                        order
                            .map(|(c, asc)| format!(
                                " order by {} {}",
                                db::ident(&c),
                                if asc { "asc" } else { "desc" }
                            ))
                            .unwrap_or_default()
                    );
                    return export(
                        table.name.clone(),
                        None,
                        db::execute_all(self.db.clone(), sql),
                    );
                }
            }
            Message::Exported(result) => match result {
                Ok(Some((path, rows, note))) => {
                    let mut text = format!("Saved {} to {path}.", count(rows as u64, "row"));
                    if let Some(note) = note {
                        text = format!("{text} {note}");
                    }
                    self.status = Some(Notice::Ok(text));
                }
                Ok(None) => {}
                Err(e) => self.status = Some(Notice::Error(format!("Export failed: {e}"))),
            },
            Message::CopyRow => {
                let row = match self.tab {
                    Tab::Data => self
                        .page
                        .ready()
                        .zip(self.selected)
                        .and_then(|(p, r)| p.result.rows.get(r).map(|row| (r, row.clone()))),
                    Tab::Query => match &self.queries.active().result {
                        Load::Ready(rs) => self
                            .queries
                            .active()
                            .selected
                            .and_then(|r| rs.rows.get(r).map(|row| (r, row.clone()))),
                        _ => None,
                    },
                    Tab::Structure | Tab::Diagram => None,
                };
                if let Some((r, row)) = row {
                    let n = if self.tab == Tab::Data {
                        self.offset + r as u64 + 1
                    } else {
                        r as u64 + 1
                    };
                    self.status = Some(Notice::Ok(format!(
                        "Copied row {} as tab-separated values.",
                        thousands(n)
                    )));
                    return iced::clipboard::write(export::tsv_row(&row));
                }
            }
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
        self.run_checked(false)
    }

    /// Runs the SQL of the active tab. Unless `confirmed`, a statement that
    /// changes or removes every row waits for the user to confirm it.
    fn run_checked(&mut self, confirmed: bool) -> Task<Message> {
        let read_only = self.config.read_only;
        let tab = self.queries.active_mut();
        let sql = tab.sql_to_run();
        if sql.trim().is_empty() || tab.running() {
            return Task::none();
        }
        // On a read-only connection the database refuses the change anyway.
        if !confirmed && !read_only {
            let risks = guard::risks(&sql);
            if !risks.is_empty() {
                tab.risks = Some(risks);
                self.tab = Tab::Query;
                return Task::none();
            }
        }
        tab.risks = None;
        tab.run += 1;
        tab.sql = sql.clone();
        tab.started = Some(std::time::Instant::now());
        tab.cancelling = false;
        tab.result = Load::Loading;
        tab.selected = None;
        let (id, run) = (tab.id, tab.run);
        self.tab = Tab::Query;
        Task::perform(
            db::execute_limited(self.db.clone(), self.config.clone(), sql),
            move |r| Message::QueryDone(id, run, r),
        )
    }

    /// Moves the selected row in the Data or Query grid, and scrolls it into view.
    fn move_selection(&mut self, step: Move) -> Task<Message> {
        let (rows, current, body) = match self.tab {
            Tab::Data => match self.page.ready() {
                Some(page) => (page.result.rows.len(), self.selected, DATA_GRID.0),
                None => return Task::none(),
            },
            Tab::Query => match &self.queries.active().result {
                Load::Ready(rs) => (rs.rows.len(), self.queries.active().selected, QUERY_GRID.0),
                _ => return Task::none(),
            },
            Tab::Structure | Tab::Diagram => return Task::none(),
        };
        if rows == 0 {
            return Task::none();
        }
        if self
            .inspector
            .as_ref()
            .is_some_and(|ins| ins.fields.iter().any(Field::changed))
            && self.tab == Tab::Data
        {
            self.status = Some(Notice::Error(
                "Save or discard the changes to this row first.".into(),
            ));
            return Task::none();
        }
        const PAGE_STEP: usize = 15;
        let last = rows - 1;
        let next = match (current, step) {
            (_, Move::Last) => last,
            (None, _) | (_, Move::First) => 0,
            (Some(r), Move::Up) => r.saturating_sub(1),
            (Some(r), Move::Down) => (r + 1).min(last),
            (Some(r), Move::PageUp) => r.saturating_sub(PAGE_STEP),
            (Some(r), Move::PageDown) => (r + PAGE_STEP).min(last),
        };
        let open = match self.tab {
            // An open row panel follows the selection.
            Tab::Data if self.inspector.is_some() => self.update(Message::SelectRow(next)),
            Tab::Data => {
                self.selected = Some(next);
                Task::none()
            }
            _ => {
                self.queries.active_mut().selected = Some(next);
                Task::none()
            }
        };
        Task::batch([open, self.reveal(body, next)])
    }

    /// Puts a grid at its first row, for new rows.
    fn scroll_to_top(&mut self, body: &'static str) -> Task<Message> {
        if let Some((y, _)) = self.scrolled.get_mut(body) {
            *y = 0.0;
        }
        operation::snap_to(body, operation::RelativeOffset::START)
    }

    /// Scrolls a grid so that a row is visible.
    fn reveal(&mut self, body: &'static str, row: usize) -> Task<Message> {
        // Until the grid reports its height, assume a common one.
        let (y, height) = match self.scrolled.get(body) {
            Some(&(y, h)) if h > 0.0 => (y, h),
            _ => (0.0, 400.0),
        };
        let top = row as f32 * grid::ROW_STEP;
        let bottom = top + grid::ROW_STEP;
        let target = if top < y {
            top
        } else if bottom > y + height {
            bottom - height
        } else {
            return Task::none();
        };
        self.scrolled.insert(body, (target, height));
        operation::scroll_to(
            body,
            operation::AbsoluteOffset {
                x: None,
                y: Some(target),
            },
        )
    }

    fn on_key(&mut self, key: Key, modifiers: keyboard::Modifiers) -> Task<Message> {
        // While a cell is edited, Esc cancels, and Tab moves to the next cell.
        if let Some((r, c, _)) = self.cell_edit {
            return match key.as_ref() {
                Key::Named(Named::Escape) => {
                    self.cell_edit = None;
                    Task::none()
                }
                Key::Character("n" | "N") if modifiers.command() && modifiers.shift() => {
                    self.update(Message::CellNull)
                }
                Key::Named(Named::Tab) => {
                    self.commit_cell();
                    let columns = self.columns.ready().map_or(0, |c| c.len());
                    let next = if modifiers.shift() {
                        c.checked_sub(1)
                    } else {
                        Some(c + 1)
                    };
                    match next.filter(|n| *n < columns) {
                        Some(n) => self.edit_cell(r, n),
                        None => Task::none(),
                    }
                }
                _ => Task::none(),
            };
        }
        match key.as_ref() {
            Key::Named(Named::Delete) if self.tab == Tab::Data => {
                self.update(Message::ToggleDeleteRow)
            }
            Key::Named(Named::F2) if self.tab == Tab::Data => match self.selected {
                Some(r) => self.edit_cell(r, 0),
                None => Task::none(),
            },
            Key::Named(Named::Escape) if self.inspector.is_some() => {
                self.update(Message::CloseInspector)
            }
            Key::Named(Named::Escape) if self.queries.active().running() => {
                self.update(Message::Cancel)
            }
            Key::Named(Named::Escape) if self.queries.active().risks.is_some() => {
                self.update(Message::DismissRisks)
            }
            Key::Named(named) if !modifiers.command() && !modifiers.alt() => {
                let step = match named {
                    Named::ArrowUp => Some(Move::Up),
                    Named::ArrowDown => Some(Move::Down),
                    Named::PageUp => Some(Move::PageUp),
                    Named::PageDown => Some(Move::PageDown),
                    Named::Home => Some(Move::First),
                    Named::End => Some(Move::Last),
                    _ => None,
                };
                match (step, named) {
                    (Some(step), _) => self.update(Message::MoveSelection(step)),
                    (None, Named::Enter) if self.tab == Tab::Data && self.inspector.is_none() => {
                        match self.selected {
                            Some(r) => self.update(Message::OpenRow(r)),
                            None => Task::none(),
                        }
                    }
                    _ => Task::none(),
                }
            }
            Key::Named(Named::Enter) if modifiers.command() && self.tab == Tab::Query => self.run(),
            Key::Character("t") if modifiers.command() => self.update(Message::NewQuery),
            Key::Character("w") if modifiers.command() && self.tab == Tab::Query => {
                let id = self.queries.active().id;
                self.update(Message::CloseQuery(id))
            }
            Key::Character("c") if modifiers.command() => self.update(Message::CopyRow),
            Key::Character("r") if modifiers.command() => self.update(Message::Refresh),
            Key::Character("p") if modifiers.command() => operation::focus("table-search"),
            Key::Character("f") if modifiers.command() && self.current.is_some() => {
                self.tab = Tab::Data;
                operation::focus("row-filter")
            }
            Key::Character("1") if modifiers.command() => {
                self.update(Message::SwitchTab(Tab::Data))
            }
            Key::Character("2") if modifiers.command() => {
                self.update(Message::SwitchTab(Tab::Structure))
            }
            Key::Character("3") if modifiers.command() => {
                self.update(Message::SwitchTab(Tab::Query))
            }
            Key::Character("4") if modifiers.command() => {
                self.update(Message::SwitchTab(Tab::Diagram))
            }
            _ => Task::none(),
        }
    }

    // ---------- view ----------

    fn view(&self, appearance: Appearance) -> Element<'_, Message> {
        let bar = top_bar(
            row![
                vrule_short(),
                fit(self.config.name.as_str())
                    .font(theme::BODY_MEDIUM)
                    .size(theme::TEXT_MD)
                    .max_width(280.0),
                fit(self.config.target())
                    .font(theme::MONO)
                    .size(theme::TEXT_XS)
                    .style(theme::text_muted)
                    .width(Fill),
                text(match self.encrypted {
                    Some(true) => "Encrypted",
                    Some(false) => "Not encrypted",
                    None => "",
                })
                .size(theme::TEXT_XS)
                .style(theme::text_muted)
                .wrapping(text::Wrapping::None),
                text(if self.config.read_only {
                    "Read-only"
                } else {
                    ""
                })
                .size(theme::TEXT_XS)
                .style(theme::text_accent)
                .wrapping(text::Wrapping::None),
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

        let middle = responsive(move |size| {
            let panel_open = match self.tab {
                Tab::Data => self.inspector.is_some(),
                Tab::Query => self.queries.show_history,
                Tab::Structure | Tab::Diagram => false,
            };
            let widths = Widths::for_width(size.width, panel_open);
            let mut middle = row![
                container(self.sidebar())
                    .width(widths.sidebar)
                    .height(Fill)
                    .style(theme::sidebar),
                vrule(),
                container(self.main(widths)).width(Fill).height(Fill),
            ];
            if self.tab == Tab::Data
                && let Some(ins) = &self.inspector
            {
                middle = middle
                    .push(vrule())
                    .push(self.inspector_view(ins, widths.panel));
            }
            middle.height(Fill).into()
        });

        column![bar, hrule(), middle, hrule(), self.status_bar()].into()
    }

    fn sidebar(&self) -> Element<'_, Message> {
        let filter = text_input("Search tables and columns", &self.filter)
            .id("table-search")
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
                // A table shows if its name matches, or if one of its columns does.
                // Then the matching columns show below the name.
                let shown: Vec<(&Table, Vec<&str>)> = tables
                    .iter()
                    .filter_map(|t| {
                        if needle.is_empty() || t.name.to_lowercase().contains(&needle) {
                            return Some((t, Vec::new()));
                        }
                        let columns: Vec<&str> = self
                            .column_index
                            .get(&(t.schema.clone(), t.name.clone()))
                            .into_iter()
                            .flatten()
                            .filter(|c| c.to_lowercase().contains(&needle))
                            .map(String::as_str)
                            .collect();
                        (!columns.is_empty()).then_some((t, columns))
                    })
                    .collect();
                if shown.is_empty() {
                    muted(format!(
                        "No table or column matches “{}”.",
                        self.filter.trim()
                    ))
                } else {
                    let many_schemas = tables.iter().any(|t| t.schema != tables[0].schema);
                    let mut list = Column::new().spacing(1);
                    let mut last_schema: Option<&Option<String>> = None;
                    for (t, columns) in shown {
                        if many_schemas && last_schema != Some(&t.schema) {
                            list = list.push(
                                container(
                                    fit(t.schema.as_deref().unwrap_or(""))
                                        .font(theme::LABEL)
                                        .size(theme::TEXT_XS)
                                        .style(theme::text_muted),
                                )
                                .padding(Padding::from([10.0, 10.0]).bottom(4.0)),
                            );
                            last_schema = Some(&t.schema);
                        }
                        let selected = self.current.as_ref() == Some(t);
                        let mut label = row![fit(t.name.as_str()).size(theme::TEXT_MD).width(Fill)]
                            .spacing(8)
                            .align_y(Center);
                        if t.is_view {
                            label = label
                                .push(text("view").size(theme::TEXT_XS).style(theme::text_muted));
                        }
                        let mut entry = column![label].spacing(2);
                        if !columns.is_empty() {
                            entry = entry.push(
                                fit(columns.join(", "))
                                    .font(theme::MONO)
                                    .size(theme::TEXT_XS)
                                    .style(theme::text_muted),
                            );
                        }
                        list = list.push(
                            button(entry)
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

    fn main(&self, widths: Widths) -> Element<'_, Message> {
        let density = widths.density();
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
            Tab::Data => self.data_tools(density, widths.main),
            Tab::Structure if self.structure_view == StructureView::Sql => small_key(
                "Copy SQL",
                theme::key,
                self.structure.ready().map(|_| Message::CopyStructureSql),
            ),
            Tab::Structure => space().into(),
            Tab::Query => self.queries.tools(density),
            Tab::Diagram => self.diagram_tools(),
        };

        let tabs = row![
            tab("Data", Tab::Data),
            tab("Structure", Tab::Structure),
            tab("Query", Tab::Query),
            tab("Diagram", Tab::Diagram),
        ]
        .spacing(4);
        // If the main area is narrow, the tools go on a line below the tabs.
        let strip: Element<_> = if density == Density::Narrow {
            column![
                tabs,
                container(tools)
                    .align_right(Fill)
                    .padding(Padding::from([6.0, 0.0])),
            ]
            .into()
        } else {
            row![
                tabs,
                space::horizontal(),
                container(tools).padding(Padding::ZERO.bottom(6.0)),
            ]
            .spacing(4)
            .align_y(iced::Bottom)
            .into()
        };
        let strip = container(strip)
            .padding(Padding::from([0.0, 12.0]).top(8.0))
            .width(Fill);

        let content = match self.tab {
            Tab::Data => self.data_view(),
            Tab::Structure => self.structure_view(),
            Tab::Diagram => self.diagram_view(),
            Tab::Query => {
                let catalog = complete::Catalog {
                    tables: self.tables.ready().map(Vec::as_slice).unwrap_or(&[]),
                    columns: &self.column_index,
                };
                self.queries.view(
                    widths.main,
                    widths.panel,
                    &catalog,
                    self.scrolled.get(QUERY_GRID.0).copied(),
                )
            }
        };

        column![strip, hrule(), content].into()
    }

    /// A hint, Arrange if tables were moved by hand, and the schema list if
    /// the database has more than one schema with tables.
    fn diagram_tools(&self) -> Element<'_, Message> {
        let Load::Ready(d) = &self.diagram else {
            return space().into();
        };
        let mut tools = row![
            text("Drag a table or the background. Ctrl and scroll to zoom.")
                .size(theme::TEXT_XS)
                .style(theme::text_muted)
                .wrapping(text::Wrapping::None)
        ]
        .spacing(8)
        .align_y(Center);
        if d.moved_by_hand() {
            tools = tools.push(small_key(
                "Arrange",
                theme::key,
                Some(Message::DiagramArrange),
            ));
        }
        if d.schemas.len() > 1 {
            tools = tools.push(
                pick_list(
                    d.schemas.as_slice(),
                    d.schema.clone(),
                    Message::DiagramSchema,
                )
                .text_size(theme::TEXT_SM)
                .padding(Padding::from([5.0, 10.0]))
                .style(theme::choice)
                .menu_style(theme::choice_menu),
            );
        }
        tools.into()
    }

    fn diagram_view(&self) -> Element<'_, Message> {
        let body: Element<_> = match &self.diagram {
            Load::Idle | Load::Loading => empty_state("Reading the tables and keys…", None),
            Load::Failed(e) => empty_state_error(e),
            Load::Ready(d) if d.is_empty() => empty_state("This database has no tables.", None),
            Load::Ready(d) => d.view(Message::DiagramOpen, Message::DiagramMoved),
        };
        container(body)
            .width(Fill)
            .height(Fill)
            .style(theme::diagram_area)
            .into()
    }

    fn data_tools(&self, density: Density, width: f32) -> Element<'_, Message> {
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
        let can_add = !self.config.read_only && self.current.as_ref().is_some_and(|t| !t.is_view);
        let short = density != Density::Full;
        let mut tools = row![].spacing(8).align_y(Center);
        // The status bar also shows the row count, so hide it here first.
        if density != Density::Narrow || width >= 440.0 {
            tools = tools.push(
                text(range)
                    .size(theme::TEXT_SM)
                    .style(theme::text_muted)
                    .wrapping(text::Wrapping::None),
            );
        }
        tools
            .push(small_key(
                if short { "Prev" } else { "Previous" },
                theme::key,
                (self.offset > 0).then_some(Message::PrevPage),
            ))
            .push(small_key(
                "Next",
                theme::key,
                (self.offset + shown < page.total).then_some(Message::NextPage),
            ))
            .push(small_key(
                if short { "Add" } else { "Add row" },
                theme::key,
                can_add.then_some(Message::AddRow),
            ))
            .push(small_key(
                if short { "Open" } else { "Open row" },
                theme::key,
                self.selected.map(Message::OpenRow),
            ))
            .push(small_key("Export", theme::key, Some(Message::ExportTable)))
            .into()
    }

    fn data_view(&self) -> Element<'_, Message> {
        let Some(table) = &self.current else {
            return empty_state("Choose a table on the left to see its rows.", None);
        };
        let rows = match &self.page {
            Load::Idle | Load::Loading => grid::skeleton(),
            Load::Failed(e) => empty_state_error(e),
            Load::Ready(page) if page.result.rows.is_empty() && self.applied_filter.is_some() => {
                empty_state(
                    "No rows match the filter.",
                    Some(small_key(
                        "Clear filter",
                        theme::key,
                        Some(Message::ClearFilter),
                    )),
                )
            }
            Load::Ready(page) if page.result.rows.is_empty() && self.offset == 0 => empty_state(
                format!("{} has no rows yet.", table.name),
                (!table.is_view && !self.config.read_only)
                    .then(|| small_key("Add row", theme::key, Some(Message::AddRow))),
            ),
            Load::Ready(page) => grid::view(grid::Grid {
                result: &page.result,
                selected: self.selected,
                on_select: Some(Message::SelectRow),
                on_sort: Some(Message::Sort),
                sort: self.sort,
                ids: DATA_GRID,
                on_scroll: Message::GridScrolled,
                viewport: self.scrolled.get(DATA_GRID.0).copied(),
                on_edit: self
                    .editable()
                    .then_some(Message::EditCell as fn(usize, usize) -> Message),
                edit: self
                    .cell_edit
                    .as_ref()
                    .map(|(row, column, value)| grid::CellEdit {
                        row: *row,
                        column: *column,
                        value,
                        on_input: Message::CellInput,
                        on_submit: Message::CommitCell,
                        on_null: self
                            .columns
                            .ready()
                            .and_then(|cols| cols.get(*column))
                            .is_some_and(|col| col.nullable)
                            .then_some(Message::CellNull),
                    }),
                changes: Some(&self.changes),
            }),
        };

        // The filter box, with a key to clear the filter while one is used.
        let filter = text_input("Filter rows, for example status = 'paid'", &self.row_filter)
            .id("row-filter")
            .on_input(Message::RowFilter)
            .on_submit(Message::ApplyFilter)
            .font(theme::MONO)
            .size(theme::TEXT_SM)
            .padding(Padding::from([6.0, 10.0]))
            .style(theme::field)
            .width(Fill);
        let mut bar = row![filter].spacing(8).align_y(Center);
        if self.applied_filter.is_some() {
            bar = bar.push(small_key("Clear", theme::key, Some(Message::ClearFilter)));
        } else {
            bar = bar.push(small_key("Filter", theme::key, Some(Message::ApplyFilter)));
        }
        column![
            container(bar).padding(Padding::from([8.0, 12.0])),
            hrule(),
            self.review_bar(),
            rows
        ]
        .into()
    }

    /// The bar with the grid changes that are not saved yet, if there are any.
    fn review_bar(&self) -> Option<Element<'_, Message>> {
        let edited = self.changes.edited.len();
        let deleted = self.changes.deleted.len();
        if self.changes.is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        if edited > 0 {
            parts.push(count(edited as u64, "changed cell"));
        }
        if deleted > 0 {
            parts.push(count(deleted as u64, "row") + " to delete");
        }
        let actions = row![
            fit(parts.join(" and "))
                .font(theme::BODY_MEDIUM)
                .size(theme::TEXT_SM)
                .width(Fill),
            quiet_toggle("Review SQL", self.show_review, Message::ToggleReview),
            small_key(
                "Discard",
                theme::key,
                (!self.saving_changes).then_some(Message::DiscardChanges)
            ),
            small_key(
                if self.saving_changes {
                    "Saving"
                } else {
                    "Save changes"
                },
                theme::key_primary,
                (!self.saving_changes).then_some(Message::SaveChanges),
            ),
        ]
        .spacing(8)
        .align_y(Center);
        let mut bar = column![actions].spacing(10);
        if let Some(e) = &self.changes_error {
            bar = bar.push(error_note(e));
        }
        if self.show_review {
            let sql = self.change_sql().join(";\n") + ";";
            bar = bar.push(
                container(
                    scrollable(
                        text(sql)
                            .font(theme::MONO)
                            .size(theme::TEXT_SM)
                            .wrapping(text::Wrapping::WordOrGlyph),
                    )
                    .style(theme::scroller),
                )
                .padding(12)
                .max_height(180)
                .width(Fill)
                .style(theme::graphite),
            );
        }
        Some(
            column![
                container(bar)
                    .padding(Padding::from([8.0, 12.0]))
                    .width(Fill)
                    .style(theme::query_strip),
                hrule()
            ]
            .into(),
        )
    }

    fn structure_view(&self) -> Element<'_, Message> {
        if self.current.is_none() {
            return empty_state("Choose a table on the left to see its columns.", None);
        }
        let ready = self.structure.ready();
        let count = |rs: fn(&db::Structure) -> &ResultSet| {
            ready
                .map(|s| format!(" ({})", rs(s).rows.len()))
                .unwrap_or_default()
        };
        let parts = [
            (StructureView::Columns, "Columns".to_string()),
            (
                StructureView::Indexes,
                format!("Indexes{}", count(|s| &s.indexes)),
            ),
            (
                StructureView::ForeignKeys,
                format!("Foreign keys{}", count(|s| &s.foreign_keys)),
            ),
            (
                StructureView::ReferencedBy,
                format!("Referenced by{}", count(|s| &s.referenced_by)),
            ),
            (StructureView::Sql, "SQL".to_string()),
        ];
        let switch = container(
            Row::with_children(parts.into_iter().map(|(view, label)| {
                button(
                    text(label)
                        .font(theme::BODY_MEDIUM)
                        .size(theme::TEXT_SM)
                        .wrapping(text::Wrapping::None),
                )
                .padding(Padding::from([5.0, 12.0]))
                .style(theme::tray_key(view == self.structure_view))
                .on_press(Message::SwitchStructure(view))
                .into()
            }))
            .spacing(2),
        )
        .padding(3)
        .style(theme::tray);
        let switch = scrollable(switch).direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::hidden(),
        ));

        let content: Element<_> = match (self.structure_view, &self.columns, &self.structure) {
            (StructureView::Columns, Load::Ready(columns), _) => structure_grid(
                &columns.listing,
                "This table has no columns.",
                self.scrolled.get(STRUCTURE_GRID.0).copied(),
            ),
            (StructureView::Columns, Load::Failed(e), _) | (_, _, Load::Failed(e)) => {
                empty_state_error(e)
            }
            (StructureView::Columns, _, _) | (_, _, Load::Idle | Load::Loading) => grid::skeleton(),
            (StructureView::Indexes, _, Load::Ready(s)) => structure_grid(
                &s.indexes,
                "This table has no indexes.",
                self.scrolled.get(STRUCTURE_GRID.0).copied(),
            ),
            (StructureView::ForeignKeys, _, Load::Ready(s)) => structure_grid(
                &s.foreign_keys,
                "This table has no foreign keys to other tables.",
                self.scrolled.get(STRUCTURE_GRID.0).copied(),
            ),
            (StructureView::ReferencedBy, _, Load::Ready(s)) => structure_grid(
                &s.referenced_by,
                "No foreign key in another table points to this table.",
                self.scrolled.get(STRUCTURE_GRID.0).copied(),
            ),
            (StructureView::Sql, _, Load::Ready(_)) => container(
                text_editor(&self.structure_sql)
                    .on_action(Message::StructureSqlAction)
                    .font(theme::MONO)
                    .size(theme::TEXT_MD)
                    .padding(16)
                    .height(Fill)
                    .highlight_with::<sql::Sql>((), sql::format)
                    .style(theme::sql_editor),
            )
            .style(theme::graphite)
            .into(),
        };
        column![
            container(switch).padding(Padding::from([8.0, 12.0])),
            hrule(),
            content
        ]
        .into()
    }

    fn inspector_view<'a>(&'a self, ins: &'a Inspector, width: f32) -> Element<'a, Message> {
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
                fit(self.current.as_ref().map(|t| t.name.as_str()).unwrap_or(""))
                    .font(theme::MONO)
                    .size(theme::TEXT_XS)
                    .style(theme::text_muted),
            ]
            .spacing(2)
            .width(Fill),
            small_key("Close", theme::ghost, Some(Message::CloseInspector)),
        ]
        .align_y(Center);

        let fields = Column::with_children(columns.iter().zip(&ins.fields).enumerate().map(
            |(i, (col, field))| {
                // The column name gets most of the line. The type gets the rest.
                let mut label = row![
                    fit(col.name.as_str())
                        .font(theme::LABEL)
                        .size(theme::TEXT_SM)
                        .style(theme::text_ink2)
                        .max_width(width * 0.55),
                    fit(col.data_type.as_str())
                        .font(theme::MONO)
                        .size(theme::TEXT_XS)
                        .style(theme::text_muted)
                        .width(Fill),
                ]
                .spacing(8)
                .align_y(Center);
                if col.primary_key {
                    label = label.push(text("key").size(theme::TEXT_XS).style(theme::text_muted));
                }
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
        let mut confirm = None;
        let footer: Element<_> = if !editable {
            muted(if self.config.read_only {
                "This connection is read-only."
            } else if self.current.as_ref().is_some_and(|t| t.is_view) {
                "Views are read-only here."
            } else {
                "This table has no primary key, so its rows are read-only here."
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
                // The confirmation has its own line, so that it fits in a narrow panel.
                confirm = Some(
                    row![
                        fit("Delete this row?")
                            .size(theme::TEXT_SM)
                            .style(theme::text_muted)
                            .width(Fill),
                        small_key("Cancel", theme::key, Some(Message::CancelDelete)),
                        small_key(
                            "Delete row",
                            theme::key_danger_armed,
                            (!ins.saving).then_some(Message::DeleteRow),
                        ),
                    ]
                    .spacing(8)
                    .align_y(Center),
                );
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
                column![ins.error.as_deref().map(error_note), confirm, footer].spacing(12),
            ]
            .spacing(16),
        )
        .width(width)
        .height(Fill)
        .padding(16)
        .style(theme::bar)
        .into()
    }

    fn status_bar(&self) -> Element<'_, Message> {
        let note: Element<_> = match &self.status {
            Some(Notice::Ok(m)) => fit(m.as_str())
                .size(theme::TEXT_XS)
                .style(theme::text_ink2)
                .width(Fill)
                .into(),
            Some(Notice::Error(e)) => fit(e.as_str())
                .size(theme::TEXT_XS)
                .style(theme::text_error)
                .width(Fill)
                .into(),
            None => space::horizontal().into(),
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
            Tab::Query => match self.queries.active().result.ready() {
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
            Tab::Diagram => match self.diagram.ready() {
                Some(d) => {
                    let (tables, keys) = d.counts();
                    vec![count(tables as u64, "table"), count(keys as u64, "key")]
                }
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
        container(row![note, right].spacing(20).align_y(Center))
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
        Message::Connected(Box::new(config.clone()), r)
    })
}

fn write(db: Db, sql: String, done: String) -> Task<Message> {
    Task::perform(db::execute(db, sql), move |r| {
        Message::RowWritten(match r {
            Ok(rs) if rs.affected == Some(0) => Err(
                "No row matched. If the row changed after this page loaded, reload the page."
                    .into(),
            ),
            Ok(_) => Ok(done.clone()),
            Err(e) => Err(e),
        })
    })
}

/// Asks where to save, then writes CSV or JSON as the file extension tells.
/// `note` is added to the message after the export.
fn export(
    name: String,
    note: Option<String>,
    rows: impl std::future::Future<Output = Result<ResultSet, String>> + Send + 'static,
) -> Task<Message> {
    Task::perform(
        async move {
            let Some(path) = export::choose_file(&name).await else {
                return Ok(None);
            };
            let rs = rows.await?;
            export::write(&path, &rs)?;
            Ok(Some((path.display().to_string(), rs.rows.len(), note)))
        },
        Message::Exported,
    )
}

/// Expands `~/` in file paths and removes spaces at the ends.
fn normalized(config: &Config) -> Config {
    let mut c = config.clone();
    c.path = normalized_path(&c.path);
    c.ca_file = normalized_path(&c.ca_file);
    c
}

fn normalized_path(path: &str) -> String {
    let path = path.trim();
    match (path.strip_prefix("~/"), dirs_next::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest).to_string_lossy().into_owned(),
        _ => path.to_string(),
    }
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

/// Puts a plain sentence before a database error if the cause is a usual one.
/// The text from the driver follows on the next line.
fn explain(error: &str) -> String {
    let lower = error.to_lowercase();
    let plain = if lower.contains("foreign key") {
        "Other rows still point at this one, so the database kept it."
    } else if lower.contains("unique") || lower.contains("duplicate key") {
        "Another row already has this value in a unique column."
    } else if lower.contains("not null") || lower.contains("null value") {
        "A required column is empty."
    } else if lower.contains("invalid input syntax") || lower.contains("datatype mismatch") {
        "A value does not match the type of its column."
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

/// The app icon, read once.
static ICON: std::sync::LazyLock<image::Handle> = std::sync::LazyLock::new(|| {
    let (rgba, width, height) = decode_png(theme::ICON_PNG).unwrap_or_default();
    image::Handle::from_rgba(width, height, rgba)
});

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

    let bar = container(
        row![
            row![
                image(ICON.clone()).width(22).height(22),
                text("sqlsea").font(theme::DISPLAY).size(theme::TEXT_LG),
            ]
            .spacing(8)
            .align_y(Center),
            container(left).width(Fill).clip(true),
            modes,
        ]
        .push(right)
        .push(
            chrome::controls(Message::Window)
                .map(|controls| row![vrule_short(), controls].spacing(12).align_y(Center)),
        )
        .spacing(12)
        .align_y(Center),
    )
    .padding(Padding::from([0.0, 16.0]).right(if chrome::CUSTOM { 10.0 } else { 16.0 }))
    .height(48)
    .center_y(48)
    .width(Fill)
    .style(theme::bar);
    chrome::title_bar(bar.into(), Message::Window)
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
        // A key label never wraps. The layout around the key gets smaller instead.
        container(
            text(label)
                .font(theme::BODY_MEDIUM)
                .size(theme::TEXT_SM)
                .wrapping(text::Wrapping::None),
        )
        .center_y(Length::Fill),
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

/// The SSH tunnel part of the PostgreSQL form.
fn ssh_fields(f: &Config) -> Element<'_, Message> {
    let toggle = column![
        checkbox(f.ssh)
            .label("Connect through an SSH tunnel")
            .on_toggle(Message::FormSsh)
            .font(theme::BODY_MEDIUM)
            .text_size(theme::TEXT_SM)
            .size(16)
            .spacing(8)
            .style(theme::check_box),
        muted(
            "Reach a database behind a server. Host and Port above are as the SSH server sees them."
        ),
    ]
    .spacing(4);
    if !f.ssh {
        return toggle.into();
    }
    let auth = container(
        Row::with_children(tunnel::SshAuth::ALL.map(|mode| {
            button(
                text(mode.label())
                    .font(theme::BODY_MEDIUM)
                    .size(theme::TEXT_SM),
            )
            .padding(Padding::from([6.0, 12.0]))
            .style(theme::tray_key(mode == f.ssh_auth))
            .on_press(Message::FormSshAuth(mode))
            .into()
        }))
        .spacing(2),
    )
    .padding(3)
    .style(theme::tray);
    let credentials: Element<_> = match f.ssh_auth {
        tunnel::SshAuth::Agent => muted("Uses the keys in your running SSH agent."),
        tunnel::SshAuth::KeyFile => column![
            labelled(
                "Private key file",
                with_browse(
                    input(
                        "~/.ssh/id_ed25519",
                        &f.ssh_key_file,
                        Message::FormSshKeyFile
                    )
                    .font(theme::MONO),
                    Message::BrowseSshKey,
                ),
            ),
            labelled(
                "Key passphrase",
                input(
                    "Only if the key has one",
                    &f.ssh_secret,
                    Message::FormSshSecret
                )
                .secure(true),
            ),
        ]
        .spacing(14)
        .into(),
        tunnel::SshAuth::Password => labelled(
            "SSH password",
            input(
                "Kept in your keyring",
                &f.ssh_secret,
                Message::FormSshSecret,
            )
            .secure(true),
        ),
    };
    let me = std::env::var("USER").unwrap_or_default();
    column![
        toggle,
        row![
            labelled(
                "SSH host",
                input("bastion.example.com", &f.ssh_host, Message::FormSshHost)
            ),
            container(labelled(
                "SSH port",
                input("22", &f.ssh_port, Message::FormSshPort)
            ))
            .width(110),
        ]
        .spacing(12),
        labelled(
            "SSH user",
            text_input(&me, &f.ssh_user)
                .on_input(Message::FormSshUser)
                .on_submit(Message::Connect)
                .padding(Padding::from([7.0, 10.0]))
                .size(theme::TEXT_MD)
                .style(theme::field),
        ),
        labelled("Sign in with", auth),
        credentials,
    ]
    .spacing(14)
    .into()
}

/// A key that looks pressed while its panel is open.
fn quiet_toggle<'a>(label: &'a str, on: bool, message: Message) -> Element<'a, Message> {
    button(
        container(
            text(label)
                .font(theme::BODY_MEDIUM)
                .size(theme::TEXT_SM)
                .wrapping(text::Wrapping::None),
        )
        .center_y(Length::Fill),
    )
    .height(theme::CONTROL_H)
    .padding(Padding::from([0.0, 12.0]))
    .style(theme::key_toggle(on))
    .on_press(message)
    .into()
}

/// A grid in the Structure tab, or a note if it has no rows.
fn structure_grid<'a>(
    rs: &'a ResultSet,
    empty: &'static str,
    viewport: Option<(f32, f32)>,
) -> Element<'a, Message> {
    if rs.rows.is_empty() {
        return empty_state(empty, None);
    }
    grid::view(grid::Grid {
        result: rs,
        selected: None,
        on_select: None,
        on_sort: None,
        sort: None,
        ids: STRUCTURE_GRID,
        on_scroll: Message::GridScrolled,
        viewport,
        on_edit: None,
        edit: None,
        changes: None,
    })
}

/// A path field with a Browse key that opens the system file dialog.
fn with_browse<'a>(
    field: text_input::TextInput<'a, Message>,
    browse: Message,
) -> Element<'a, Message> {
    row![
        field.width(Fill),
        small_key("Browse", theme::key, Some(browse))
    ]
    .spacing(8)
    .align_y(Center)
    .into()
}

#[derive(Debug, Clone, Copy)]
enum FileField {
    Database,
    CaFile,
    SshKey,
}

/// Opens the system file dialog in the folder of the current path, if any.
fn pick_file(field: FileField, current: &str) -> Task<Message> {
    let mut dialog = rfd::AsyncFileDialog::new();
    dialog = match field {
        // SQLite files often have no extension, so the dialog shows all files.
        FileField::Database => dialog.set_title("Choose a SQLite database"),
        FileField::CaFile => dialog
            .set_title("Choose a CA certificate")
            .add_filter("Certificates", &["pem", "crt", "cer"]),
        // Private key files usually have no extension.
        FileField::SshKey => dialog.set_title("Choose an SSH private key"),
    };
    let folder = std::path::Path::new(normalized_path(current).as_str())
        .parent()
        .filter(|p| p.is_dir())
        .map(std::path::Path::to_path_buf)
        .or_else(dirs_next::home_dir);
    if let Some(folder) = folder {
        dialog = dialog.set_directory(folder);
    }
    Task::perform(
        async move {
            dialog
                .pick_file()
                .await
                .map(|file| file.path().display().to_string())
        },
        move |path| Message::FilePicked(field, path),
    )
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
    text(s)
        .size(theme::TEXT_SM)
        .style(theme::text_muted)
        .wrapping(text::Wrapping::WordOrGlyph)
        .into()
}

/// An error box. A plain sentence comes first if there is one, then the database text.
fn error_note<'a>(message: &str) -> Element<'a, Message> {
    let body: Element<_> = match message.split_once('\n') {
        Some((plain, raw)) => column![
            text(plain.to_string())
                .font(theme::BODY_MEDIUM)
                .size(theme::TEXT_SM)
                .wrapping(text::Wrapping::WordOrGlyph),
            text(raw.to_string())
                .font(theme::MONO)
                .size(theme::TEXT_XS)
                .wrapping(text::Wrapping::WordOrGlyph),
        ]
        .spacing(4)
        .into(),
        None => text(message.to_string())
            .font(theme::MONO)
            .size(theme::TEXT_SM)
            .wrapping(text::Wrapping::WordOrGlyph)
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
    let mut col = column![
        text(message)
            .size(theme::TEXT_MD)
            .style(theme::text_muted)
            .wrapping(text::Wrapping::WordOrGlyph)
    ]
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

/// The width class of the main area. It sets what the toolbar shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Density {
    /// All controls, with full labels.
    Full,
    /// Shorter labels and fewer hints.
    Compact,
    /// The toolbar is on its own line below the tabs.
    Narrow,
}

/// The column widths of the session screen, from the window width.
#[derive(Debug, Clone, Copy)]
struct Widths {
    sidebar: f32,
    /// The row panel or the history panel, when open.
    panel: f32,
    main: f32,
}

impl Widths {
    fn for_width(total: f32, panel_open: bool) -> Self {
        let sidebar = (total * 0.2).clamp(170.0, 260.0);
        let panel = if panel_open {
            (total * 0.3).clamp(260.0, 340.0)
        } else {
            0.0
        };
        Widths {
            sidebar,
            panel,
            main: (total - sidebar - panel - 2.0).max(0.0),
        }
    }

    fn density(self) -> Density {
        match self.main {
            w if w >= 780.0 => Density::Full,
            w if w >= 620.0 => Density::Compact,
            _ => Density::Narrow,
        }
    }
}

// ---------- screenshots for testing (cargo run --features shot) ----------

/// `SQLSEA_SCRIPT="table:users;row:2;shot:/tmp/a.png;quit"` runs one step
/// every 1.2 seconds and saves screenshots of the window.
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
            "row" => arg.parse().ok().map(Message::OpenRow),
            "select" => arg.parse().ok().map(Message::SelectRow),
            "editcell" => arg
                .split_once(',')
                .and_then(|(r, c)| Some(Message::EditCell(r.parse().ok()?, c.parse().ok()?))),
            "cellvalue" => Some(Message::CellInput(arg.to_string())),
            "commitcell" => Some(Message::CommitCell),
            "cellnull" => Some(Message::CellNull),
            "markdelete" => Some(Message::ToggleDeleteRow),
            "review" => Some(Message::ToggleReview),
            "savechanges" => Some(Message::SaveChanges),
            "discardchanges" => Some(Message::DiscardChanges),
            "add" => Some(Message::AddRow),
            "tab" => Some(Message::SwitchTab(match arg {
                "structure" => Tab::Structure,
                "query" => Tab::Query,
                "diagram" => Tab::Diagram,
                _ => Tab::Data,
            })),
            "sql" => Some(Message::SetSql(arg.replace("\\n", "\n"))),
            "schema" => Some(Message::DiagramSchema(arg.to_string())),
            "arrange" => Some(Message::DiagramArrange),
            "run" => Some(Message::Run),
            "pick" => arg.parse().ok().map(Message::PickSaved),
            "kind" => Some(Message::FormKind(if arg == "sqlite" {
                Kind::Sqlite
            } else {
                Kind::Postgres
            })),
            "connect" => Some(Message::Connect),
            "move" => Some(Message::MoveSelection(match arg {
                "up" => Move::Up,
                "pageup" => Move::PageUp,
                "pagedown" => Move::PageDown,
                "first" => Move::First,
                "last" => Move::Last,
                _ => Move::Down,
            })),
            "filter" => Some(Message::RowFilter(arg.to_string())),
            "applyfilter" => Some(Message::ApplyFilter),
            "anyway" => Some(Message::RunAnyway),
            "readonly" => Some(Message::FormReadOnly(arg != "off")),
            "url" => Some(Message::FormUrl(arg.to_string())),
            "applyurl" => Some(Message::ApplyUrl),
            "search" => Some(Message::Filter(arg.to_string())),
            "formend" => Some(Message::ScrollFormEnd),
            "scrollquery" => arg.parse().ok().map(Message::ScrollQueryGrid),
            "structure" => Some(Message::SwitchStructure(match arg {
                "indexes" => StructureView::Indexes,
                "keys" => StructureView::ForeignKeys,
                "refs" => StructureView::ReferencedBy,
                "sql" => StructureView::Sql,
                _ => StructureView::Columns,
            })),
            "complete" => arg.parse().ok().map(Message::Complete),
            "mode" => Some(Message::SetAppearance(match arg {
                "dark" => Appearance::Dark,
                "light" => Appearance::Light,
                _ => Appearance::Auto,
            })),
            "forget" => Some(Message::DeleteSaved),
            "newtab" => Some(Message::NewQuery),
            "closetab" => arg.parse().ok().map(Message::CloseQuery),
            "history" => Some(Message::ToggleHistory),
            "usehist" => arg.parse().ok().map(Message::UseHistory),
            "cancel" => Some(Message::Cancel),
            "copy" => Some(Message::CopyRow),
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
