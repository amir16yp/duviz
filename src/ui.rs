use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::scan::{human_size, Entry, ExtFile, ExtStat};
use crate::treemap::{self, Rect as TRect};

const PALETTE: [Color; 12] = [
    Color::Rgb(70, 130, 180),
    Color::Rgb(60, 179, 113),
    Color::Rgb(218, 165, 32),
    Color::Rgb(205, 92, 92),
    Color::Rgb(147, 112, 219),
    Color::Rgb(72, 209, 204),
    Color::Rgb(255, 140, 0),
    Color::Rgb(100, 149, 237),
    Color::Rgb(46, 139, 87),
    Color::Rgb(210, 105, 30),
    Color::Rgb(123, 104, 238),
    Color::Rgb(32, 178, 170),
];

pub struct App {
    pub root: Entry,
    /// Stack of child indices from root to current directory.
    pub path: Vec<usize>,
    pub selected: usize,
    pub list_state: ListState,
    pub ext_selected: usize,
    pub ext_list_state: ListState,
    /// Focus: false = files pane, true = extensions pane.
    pub focus_ext: bool,
    /// When set, extensions pane lists matching file paths instead of aggregates.
    pub ext_drill: Option<String>,
    /// Full size-sorted match list (built on Enter).
    pub ext_files: Vec<ExtFile>,
    pub ext_file_selected: usize,
    /// First visible row index into `ext_files` (viewport scroll).
    pub ext_file_scroll: usize,
    /// Visible row count of the focused list (for PageUp/PageDown).
    pub page_rows: usize,
    pub use_colors: bool,
}

const EXT_FILE_TREEMAP_CAP: usize = 256;

impl App {
    pub fn new(root: Entry, use_colors: bool) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        let mut ext_list_state = ListState::default();
        ext_list_state.select(Some(0));
        Self {
            root,
            path: Vec::new(),
            selected: 0,
            list_state,
            ext_selected: 0,
            ext_list_state,
            focus_ext: false,
            ext_drill: None,
            ext_files: Vec::new(),
            ext_file_selected: 0,
            ext_file_scroll: 0,
            page_rows: 10,
            use_colors,
        }
    }

    fn page_step(&self) -> usize {
        self.page_rows.max(1)
    }

    pub fn page_down(&mut self) {
        let step = self.page_step();
        if self.focus_ext {
            if self.ext_drill.is_some() {
                let len = self.ext_files.len();
                if len == 0 {
                    return;
                }
                self.ext_file_selected = (self.ext_file_selected + step).min(len - 1);
            } else {
                let len = self.extensions().len();
                if len == 0 {
                    return;
                }
                self.ext_selected = (self.ext_selected + step).min(len - 1);
                self.ext_list_state.select(Some(self.ext_selected));
            }
        } else {
            let len = self.children().len();
            if len == 0 {
                return;
            }
            self.selected = (self.selected + step).min(len - 1);
            self.list_state.select(Some(self.selected));
        }
    }

    pub fn page_up(&mut self) {
        let step = self.page_step();
        if self.focus_ext {
            if self.ext_drill.is_some() {
                if self.ext_files.is_empty() {
                    return;
                }
                self.ext_file_selected = self.ext_file_selected.saturating_sub(step);
            } else {
                if self.extensions().is_empty() {
                    return;
                }
                self.ext_selected = self.ext_selected.saturating_sub(step);
                self.ext_list_state.select(Some(self.ext_selected));
            }
        } else {
            if self.children().is_empty() {
                return;
            }
            self.selected = self.selected.saturating_sub(step);
            self.list_state.select(Some(self.selected));
        }
    }

    pub fn current(&self) -> &Entry {
        let mut node = &self.root;
        for &idx in &self.path {
            node = &node.children[idx];
        }
        node
    }

    fn children(&self) -> &[Entry] {
        &self.current().children
    }

    pub fn extensions(&self) -> Vec<ExtStat> {
        self.current().extension_stats()
    }

    pub fn toggle_focus(&mut self) {
        self.focus_ext = !self.focus_ext;
    }

    pub fn select_next(&mut self) {
        if self.focus_ext {
            if self.ext_drill.is_some() {
                let len = self.ext_files.len();
                if len == 0 {
                    return;
                }
                self.ext_file_selected = (self.ext_file_selected + 1) % len;
            } else {
                let len = self.extensions().len();
                if len == 0 {
                    return;
                }
                self.ext_selected = (self.ext_selected + 1) % len;
                self.ext_list_state.select(Some(self.ext_selected));
            }
        } else {
            let len = self.children().len();
            if len == 0 {
                return;
            }
            self.selected = (self.selected + 1) % len;
            self.list_state.select(Some(self.selected));
        }
    }

    pub fn select_prev(&mut self) {
        if self.focus_ext {
            if self.ext_drill.is_some() {
                let len = self.ext_files.len();
                if len == 0 {
                    return;
                }
                self.ext_file_selected = if self.ext_file_selected == 0 {
                    len - 1
                } else {
                    self.ext_file_selected - 1
                };
            } else {
                let len = self.extensions().len();
                if len == 0 {
                    return;
                }
                self.ext_selected = if self.ext_selected == 0 {
                    len - 1
                } else {
                    self.ext_selected - 1
                };
                self.ext_list_state.select(Some(self.ext_selected));
            }
        } else {
            let len = self.children().len();
            if len == 0 {
                return;
            }
            self.selected = if self.selected == 0 {
                len - 1
            } else {
                self.selected - 1
            };
            self.list_state.select(Some(self.selected));
        }
    }

    pub fn enter(&mut self) {
        if self.focus_ext {
            if self.ext_drill.is_some() {
                return;
            }
            let stats = self.extensions();
            let Some(stat) = stats.get(self.ext_selected) else {
                return;
            };
            let ext = stat.ext.clone();
            self.ext_files = self.current().files_with_extension(&ext);
            self.ext_drill = Some(ext);
            self.ext_file_selected = 0;
            self.ext_file_scroll = 0;
            return;
        }
        let kids = self.children();
        if kids.is_empty() {
            return;
        }
        if !kids[self.selected].is_dir || kids[self.selected].children.is_empty() {
            return;
        }
        self.path.push(self.selected);
        self.selected = 0;
        self.list_state.select(Some(0));
        self.reset_ext_selection();
        self.clear_ext_drill();
    }

    pub fn leave(&mut self) {
        if self.focus_ext {
            if self.ext_drill.is_some() {
                self.clear_ext_drill();
                self.ext_list_state.select(Some(self.ext_selected));
            }
            return;
        }
        if let Some(idx) = self.path.pop() {
            self.selected = idx;
            self.list_state.select(Some(idx));
            self.reset_ext_selection();
            self.clear_ext_drill();
        }
    }

    fn clear_ext_drill(&mut self) {
        self.ext_drill = None;
        self.ext_files.clear();
        self.ext_file_selected = 0;
        self.ext_file_scroll = 0;
    }

    fn reset_ext_selection(&mut self) {
        self.ext_selected = 0;
        self.ext_list_state.select(Some(0));
    }

    pub fn select_home(&mut self) {
        if self.focus_ext {
            if self.ext_drill.is_some() {
                self.ext_file_selected = 0;
                self.ext_file_scroll = 0;
            } else {
                self.ext_selected = 0;
                self.ext_list_state.select(Some(0));
            }
        } else {
            self.selected = 0;
            self.list_state.select(Some(0));
        }
    }

    pub fn select_end(&mut self) {
        if self.focus_ext {
            if self.ext_drill.is_some() {
                let len = self.ext_files.len();
                if len > 0 {
                    self.ext_file_selected = len - 1;
                }
            } else {
                let len = self.extensions().len();
                if len > 0 {
                    self.ext_selected = len - 1;
                    self.ext_list_state.select(Some(self.ext_selected));
                }
            }
        } else {
            let len = self.children().len();
            if len > 0 {
                self.selected = len - 1;
                self.list_state.select(Some(self.selected));
            }
        }
    }

    pub fn location(&self) -> String {
        self.current().path.display().to_string()
    }
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Percentage(62),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(frame, app, chunks[0]);
    draw_tree_body(frame, app, chunks[1]);
    draw_ext_body(frame, app, chunks[2]);
    draw_footer(frame, chunks[3], app.use_colors);
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let cur = app.current();
    let title = format!(" duviz · {} · {} ", app.location(), human_size(cur.size));
    let style = if app.use_colors {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };
    frame.render_widget(Paragraph::new(Line::from(Span::styled(title, style))), area);
}

fn draw_footer(frame: &mut Frame, area: Rect, use_colors: bool) {
    let help = " ↑/↓ move · PgUp/PgDn page · Tab focus · Enter open · Backspace up · q quit ";
    let style = if use_colors {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };
    frame.render_widget(Paragraph::new(Line::from(Span::styled(help, style))), area);
}

fn draw_tree_body(frame: &mut Frame, app: &mut App, area: Rect) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);

    draw_file_list(frame, app, panes[0]);
    let kids = &app.current().children;
    let sizes: Vec<u64> = kids.iter().map(|e| e.size).collect();
    let labels: Vec<String> = kids
        .iter()
        .map(|e| {
            if e.is_dir {
                format!("{}/", e.name)
            } else {
                e.name.clone()
            }
        })
        .collect();
    let selected = if app.focus_ext {
        None
    } else {
        Some(app.selected)
    };
    draw_treemap_panel(
        frame,
        panes[1],
        " Treemap ",
        &sizes,
        &labels,
        selected,
        app.use_colors,
    );
}

fn draw_ext_body(frame: &mut Frame, app: &mut App, area: Rect) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);

    if app.ext_drill.is_some() {
        draw_ext_file_list(frame, app, panes[0]);
        let n = app.ext_files.len().min(EXT_FILE_TREEMAP_CAP);
        let sizes: Vec<u64> = app.ext_files[..n].iter().map(|e| e.size).collect();
        let labels: Vec<String> = app.ext_files[..n]
            .iter()
            .map(|e| {
                e.path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| e.path.display().to_string())
            })
            .collect();
        let selected = if app.focus_ext && app.ext_file_selected < n {
            Some(app.ext_file_selected)
        } else {
            None
        };
        let title = match &app.ext_drill {
            Some(ext) => format!(" {ext} files "),
            None => " Files ".to_string(),
        };
        draw_treemap_panel(
            frame,
            panes[1],
            &title,
            &sizes,
            &labels,
            selected,
            app.use_colors,
        );
    } else {
        let stats = app.extensions();
        draw_ext_list(frame, app, panes[0], &stats);

        let sizes: Vec<u64> = stats.iter().map(|e| e.size).collect();
        let labels: Vec<String> = stats.iter().map(|e| e.ext.clone()).collect();
        let selected = if app.focus_ext {
            Some(app.ext_selected)
        } else {
            None
        };
        draw_treemap_panel(
            frame,
            panes[1],
            " Extensions treemap ",
            &sizes,
            &labels,
            selected,
            app.use_colors,
        );
    }
}

fn focused_title(base: &str, focused: bool) -> String {
    if focused {
        format!(" {base} ● ")
    } else {
        format!(" {base} ")
    }
}

fn draw_file_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = !app.focus_ext;
    if focused {
        app.page_rows = area.height.saturating_sub(2) as usize;
    }
    let selected = app.selected;
    let use_colors = app.use_colors;
    let cur = app.current();
    let total = cur.size.max(1);
    let kids = &cur.children;

    let items: Vec<ListItem> = kids
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let pct = (e.size as f64 / total as f64) * 100.0;
            let marker = if e.is_dir { "/" } else { "" };
            let label = format!(
                "{:>8} {:>5.1}%  {}{}",
                human_size(e.size),
                pct,
                e.name,
                marker
            );
            let mut style = Style::default();
            if use_colors {
                style = style.fg(PALETTE[i % PALETTE.len()]);
            }
            if focused && i == selected {
                style = style.add_modifier(Modifier::REVERSED | Modifier::BOLD);
            }
            ListItem::new(label).style(style)
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(focused_title("Largest", focused));
    let list = List::new(items).block(block);
    frame.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_ext_list(frame: &mut Frame, app: &mut App, area: Rect, stats: &[ExtStat]) {
    let total: u64 = stats.iter().map(|s| s.size).sum::<u64>().max(1);
    let focused = app.focus_ext;
    if focused {
        app.page_rows = area.height.saturating_sub(2) as usize;
    }

    let items: Vec<ListItem> = stats
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let pct = (e.size as f64 / total as f64) * 100.0;
            let files = if e.count == 1 {
                "1 file".to_string()
            } else {
                format!("{} files", e.count)
            };
            let label = format!(
                "{:>8} {:>5.1}%  {:<16} {}",
                human_size(e.size),
                pct,
                e.ext,
                files
            );
            let mut style = Style::default();
            if app.use_colors {
                style = style.fg(PALETTE[i % PALETTE.len()]);
            }
            if focused && i == app.ext_selected {
                style = style.add_modifier(Modifier::REVERSED | Modifier::BOLD);
            }
            ListItem::new(label).style(style)
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(focused_title("Extensions", focused));
    let list = List::new(items).block(block);
    frame.render_stateful_widget(list, area, &mut app.ext_list_state);
}

fn draw_ext_file_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus_ext;
    let use_colors = app.use_colors;
    let selected = app.ext_file_selected;
    let ext = app.ext_drill.clone().unwrap_or_else(|| "?".to_string());
    let root = app.current().path.clone();
    let total_matches = app.ext_files.len();
    let total_bytes: u64 = app.ext_files.iter().map(|f| f.size).sum::<u64>().max(1);

    let base = if total_matches == 0 {
        format!("{ext} paths · 0")
    } else {
        format!("{ext} paths · {}/{total_matches}", selected + 1)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(focused_title(&base, focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let view_h = inner.height as usize;
    if focused {
        app.page_rows = view_h;
    }
    if view_h == 0 || total_matches == 0 {
        return;
    }

    // Keep selection inside the terminal viewport.
    if selected < app.ext_file_scroll {
        app.ext_file_scroll = selected;
    }
    if selected >= app.ext_file_scroll + view_h {
        app.ext_file_scroll = selected + 1 - view_h;
    }
    let max_scroll = total_matches.saturating_sub(view_h);
    if app.ext_file_scroll > max_scroll {
        app.ext_file_scroll = max_scroll;
    }

    let start = app.ext_file_scroll;
    let end = (start + view_h).min(total_matches);
    let mut lines: Vec<Line> = Vec::with_capacity(end - start);
    for i in start..end {
        let e = &app.ext_files[i];
        let pct = (e.size as f64 / total_bytes as f64) * 100.0;
        let path = e
            .path
            .strip_prefix(&root)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| e.path.display().to_string());
        let label = format!("{:>8} {:>5.1}%  {path}", human_size(e.size), pct);
        let mut style = Style::default();
        if use_colors {
            style = style.fg(PALETTE[i % PALETTE.len()]);
        }
        if focused && i == selected {
            style = style.add_modifier(Modifier::REVERSED | Modifier::BOLD);
        }
        lines.push(Line::from(Span::styled(label, style)));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_treemap_panel(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    sizes: &[u64],
    labels: &[String],
    selected: Option<usize>,
    use_colors: bool,
) {
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Clear, inner);

    if sizes.is_empty() || inner.width < 2 || inner.height < 1 {
        frame.render_widget(Paragraph::new("(empty)"), inner);
        return;
    }

    let bounds = TRect {
        x: 0,
        y: 0,
        w: inner.width,
        h: inner.height,
    };
    let tiles = treemap::layout(sizes, bounds);

    let mut grid: Vec<Vec<Option<usize>>> =
        vec![vec![None; inner.width as usize]; inner.height as usize];
    for tile in &tiles {
        let r = tile.rect;
        for dy in 0..r.h {
            for dx in 0..r.w {
                let y = (r.y + dy) as usize;
                let x = (r.x + dx) as usize;
                if y < grid.len() && x < grid[0].len() {
                    grid[y][x] = Some(tile.index);
                }
            }
        }
    }

    for y in 0..inner.height {
        let mut spans = Vec::new();
        let row = &grid[y as usize];
        let mut x = 0u16;
        while x < inner.width {
            let idx = row[x as usize];
            let mut run = 1u16;
            while x + run < inner.width && row[(x + run) as usize] == idx {
                run += 1;
            }

            let style = match idx {
                Some(i) => {
                    let base = if use_colors {
                        PALETTE[i % PALETTE.len()]
                    } else {
                        Color::Gray
                    };
                    let is_sel = selected == Some(i);
                    if is_sel {
                        Style::default()
                            .fg(Color::Black)
                            .bg(base)
                            .add_modifier(Modifier::BOLD)
                    } else if use_colors {
                        Style::default().fg(Color::Black).bg(base)
                    } else {
                        Style::default().fg(Color::White).bg(Color::DarkGray)
                    }
                }
                None => Style::default(),
            };

            let text = match idx {
                Some(i) if is_tile_origin(y, x, &tiles, i) => {
                    let name = labels.get(i).map(String::as_str).unwrap_or("");
                    pad_label(&truncate(name, run as usize), run as usize)
                }
                _ => " ".repeat(run as usize),
            };

            spans.push(Span::styled(text, style));
            x += run;
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect {
                x: inner.x,
                y: inner.y + y,
                width: inner.width,
                height: 1,
            },
        );
    }
}

fn is_tile_origin(y: u16, x: u16, tiles: &[treemap::Tile], index: usize) -> bool {
    tiles
        .iter()
        .find(|t| t.index == index)
        .map(|t| t.rect.x == x && t.rect.y == y)
        .unwrap_or(false)
}

fn truncate(name: &str, width: usize) -> String {
    if width < 1 {
        return String::new();
    }
    name.chars().take(width).collect()
}

fn pad_label(name: &str, width: usize) -> String {
    let count = name.chars().count();
    if count >= width {
        name.chars().take(width).collect()
    } else {
        format!("{name}{}", " ".repeat(width - count))
    }
}
