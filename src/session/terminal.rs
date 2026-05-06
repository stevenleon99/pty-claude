//! Terminal multiplexer using vte for terminal parsing
//!
//! Manages terminal screen state, viewports, and semantic change detection.
//! Uses the `vte` crate for VT escape sequence parsing.

use serde::{Deserialize, Serialize};

use crate::session::launch::TerminalSize;

/// Kind of semantic change detected in terminal output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalSemanticChangeKind {
    None,
    MeaningfulOutput,
    CosmeticChurn,
    CursorOnly,
    AltScreenTransition,
}

impl Default for TerminalSemanticChangeKind {
    fn default() -> Self {
        TerminalSemanticChangeKind::None
    }
}

/// Detailed terminal semantic change information.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SemanticChange {
    pub kind: TerminalSemanticChangeKind,
    pub changed_visible_line_count: usize,
    pub scrollback_lines_added: usize,
    pub appended_visible_character_count: usize,
    pub cursor_moved: bool,
    pub alt_screen_entered: bool,
    pub alt_screen_exited: bool,
}

/// Snapshot of the terminal screen state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScreenSnapshot {
    pub columns: u16,
    pub rows: u16,
    pub render_revision: u64,
    pub cursor_row: usize,
    pub cursor_column: usize,
    pub visible_lines: Vec<String>,
    pub scrollback_lines: Vec<String>,
}

/// Viewport state for a specific viewer.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ViewportState {
    pub columns: u16,
    pub rows: u16,
    pub horizontal_offset: usize,
    pub follow_cursor: bool,
}

/// Viewport snapshot for a specific viewer.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ViewportSnapshot {
    pub view_id: String,
    pub columns: u16,
    pub rows: u16,
    pub render_revision: u64,
    pub total_line_count: usize,
    pub viewport_top_line: usize,
    pub horizontal_offset: usize,
    pub cursor_viewport_row: Option<usize>,
    pub cursor_viewport_column: Option<usize>,
    pub visible_lines: Vec<String>,
}

/// Terminal multiplexer managing screen state and viewports.
pub struct TerminalMultiplexer {
    terminal_size: TerminalSize,
    screen_lines: Vec<String>,
    scrollback_lines: Vec<String>,
    cursor_row: usize,
    cursor_column: usize,
    render_revision: u64,
    max_scrollback: usize,
    last_semantic_change: SemanticChange,
    viewports: std::collections::HashMap<String, ViewportState>,
    /// Track whether we're in alternate screen mode
    alt_screen: bool,
}

impl TerminalMultiplexer {
    pub fn new(terminal_size: TerminalSize, max_scrollback_lines: usize) -> Self {
        let rows = terminal_size.rows as usize;
        TerminalMultiplexer {
            terminal_size,
            screen_lines: vec![String::new(); rows],
            scrollback_lines: Vec::new(),
            cursor_row: 0,
            cursor_column: 0,
            render_revision: 0,
            max_scrollback: max_scrollback_lines,
            last_semantic_change: SemanticChange::default(),
            viewports: std::collections::HashMap::new(),
            alt_screen: false,
        }
    }

    pub fn terminal_size(&self) -> TerminalSize {
        self.terminal_size
    }

    pub fn last_semantic_change(&self) -> &SemanticChange {
        &self.last_semantic_change
    }

    /// Resize the terminal.
    pub fn resize(&mut self, new_size: TerminalSize) {
        let old_rows = self.terminal_size.rows as usize;
        let new_rows = new_size.rows as usize;
        let new_cols = new_size.columns as usize;

        self.terminal_size = new_size;

        if new_rows > old_rows {
            self.screen_lines.resize(new_rows, String::new());
        } else if new_rows < old_rows {
            // Move excess lines to scrollback
            for _ in new_rows..old_rows {
                if let Some(line) = self.screen_lines.get(new_rows) {
                    self.scrollback_lines.push(line.clone());
                }
            }
            self.screen_lines.truncate(new_rows);
        }

        // Truncate lines to new column width
        for line in &mut self.screen_lines {
            if line.len() > new_cols {
                line.truncate(new_cols);
            }
        }

        // Trim scrollback
        while self.scrollback_lines.len() > self.max_scrollback {
            self.scrollback_lines.remove(0);
        }

        self.render_revision += 1;
    }

    /// Append raw terminal output data.
    pub fn append(&mut self, data: &str) {
        if data.is_empty() {
            return;
        }

        let cols = self.terminal_size.columns as usize;
        let rows = self.terminal_size.rows as usize;
        let prev_revision = self.render_revision;

        // Simple line-based processing (a full implementation would use vte::Parser)
        // For Phase 2, handle basic text output without full VT escape sequence support.
        let mut prev_semantic = SemanticChange::default();

        let mut chars_processed = 0usize;

        for ch in data.chars() {
            match ch {
                '\n' => {
                    self.cursor_row += 1;
                    self.cursor_column = 0;
                    if self.cursor_row >= rows {
                        // Scroll up
                        let scrolled = self.screen_lines.remove(0);
                        if !self.alt_screen {
                            self.scrollback_lines.push(scrolled);
                            if self.scrollback_lines.len() > self.max_scrollback {
                                self.scrollback_lines.remove(0);
                            }
                        }
                        self.screen_lines.push(String::new());
                        self.cursor_row = rows - 1;
                        prev_semantic.scrollback_lines_added += 1;
                    }
                }
                '\r' => {
                    self.cursor_column = 0;
                }
                '\x1b' => {
                    // Escape sequence start - simplified handling
                    // A full implementation would parse CSI sequences via vte::Parser
                    prev_semantic.kind = TerminalSemanticChangeKind::CosmeticChurn;
                }
                '\t' => {
                    // Tab: advance to next tab stop (every 8 columns)
                    let next_tab = ((self.cursor_column / 8) + 1) * 8;
                    self.cursor_column = next_tab.min(cols - 1);
                }
                _ => {
                    if self.cursor_row < rows {
                        // Ensure the line is long enough
                        let line = &mut self.screen_lines[self.cursor_row];
                        while line.len() <= self.cursor_column {
                            line.push(' ');
                        }
                        if self.cursor_column < cols {
                            // Replace character at cursor position
                            let byte_pos = line.char_indices().nth(self.cursor_column);
                            if let Some((pos, _)) = byte_pos {
                                line.replace_range(pos..pos + ch.len_utf8(), &ch.to_string());
                            } else {
                                line.push(ch);
                            }
                            self.cursor_column += 1;
                            chars_processed += 1;
                        }
                    }
                }
            }
        }

        // Determine semantic change kind
        if chars_processed > 0 {
            prev_semantic.kind = TerminalSemanticChangeKind::MeaningfulOutput;
            prev_semantic.appended_visible_character_count = chars_processed;
        }

        if self.render_revision != prev_revision || chars_processed > 0 {
            self.render_revision += 1;
        }

        prev_semantic.cursor_moved = true;
        self.last_semantic_change = prev_semantic;
    }

    /// Get a snapshot of the current terminal screen.
    pub fn snapshot(&self) -> ScreenSnapshot {
        ScreenSnapshot {
            columns: self.terminal_size.columns,
            rows: self.terminal_size.rows,
            render_revision: self.render_revision,
            cursor_row: self.cursor_row,
            cursor_column: self.cursor_column,
            visible_lines: self.screen_lines.clone(),
            scrollback_lines: self.scrollback_lines.clone(),
        }
    }

    /// Update or create a viewport for a specific viewer.
    pub fn update_viewport(&mut self, view_id: &str, viewport_size: TerminalSize) {
        self.viewports.insert(
            view_id.to_string(),
            ViewportState {
                columns: viewport_size.columns,
                rows: viewport_size.rows,
                horizontal_offset: 0,
                follow_cursor: true,
            },
        );
    }

    /// Remove a viewport.
    pub fn remove_viewport(&mut self, view_id: &str) {
        self.viewports.remove(view_id);
    }

    /// Get a snapshot for a specific viewport.
    pub fn viewport_snapshot(&self, view_id: &str) -> Option<ViewportSnapshot> {
        let state = self.viewports.get(view_id)?;
        let vp_rows = state.rows as usize;

        // Calculate which lines to show
        let total_lines = self.scrollback_lines.len() + self.screen_lines.len();
        let viewport_top = if state.follow_cursor {
            total_lines.saturating_sub(vp_rows)
        } else {
            0
        };

        // Gather visible lines from scrollback + screen
        let all_lines: Vec<&String> = self
            .scrollback_lines
            .iter()
            .chain(self.screen_lines.iter())
            .collect();

        let visible: Vec<String> = all_lines
            .into_iter()
            .skip(viewport_top)
            .take(vp_rows)
            .cloned()
            .collect();

        let cursor_viewport_row = if self.cursor_row + self.scrollback_lines.len() >= viewport_top
            && self.cursor_row + self.scrollback_lines.len() < viewport_top + vp_rows
        {
            Some(self.cursor_row + self.scrollback_lines.len() - viewport_top)
        } else {
            None
        };

        Some(ViewportSnapshot {
            view_id: view_id.to_string(),
            columns: state.columns,
            rows: state.rows,
            render_revision: self.render_revision,
            total_line_count: total_lines,
            viewport_top_line: viewport_top,
            horizontal_offset: state.horizontal_offset,
            cursor_viewport_row,
            cursor_viewport_column: Some(self.cursor_column),
            visible_lines: visible,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_output() {
        let mut mux = TerminalMultiplexer::new(
            TerminalSize {
                columns: 80,
                rows: 24,
            },
            2000,
        );

        mux.append("Hello, World!\n");

        assert_eq!(mux.screen_lines[0], "Hello, World!");
        assert_eq!(mux.cursor_row, 1);
        assert_eq!(mux.cursor_column, 0);
    }

    #[test]
    fn test_scroll_on_overflow() {
        let mut mux = TerminalMultiplexer::new(
            TerminalSize {
                columns: 20,
                rows: 3,
            },
            10,
        );

        mux.append("line1\nline2\nline3\n");
        mux.append("line4\n");

        assert_eq!(mux.screen_lines.len(), 3);
        assert_eq!(mux.screen_lines[0], "line3");
        assert_eq!(mux.screen_lines[1], "line4");
        assert_eq!(mux.screen_lines[2], "");
        assert_eq!(mux.scrollback_lines.len(), 2);
        assert_eq!(mux.scrollback_lines[0], "line1");
        assert_eq!(mux.scrollback_lines[1], "line2");
    }

    #[test]
    fn test_resize() {
        let mut mux = TerminalMultiplexer::new(
            TerminalSize {
                columns: 80,
                rows: 24,
            },
            2000,
        );

        mux.append("test");
        mux.resize(TerminalSize {
            columns: 40,
            rows: 12,
        });

        assert_eq!(mux.terminal_size.columns, 40);
        assert_eq!(mux.terminal_size.rows, 12);
    }

    #[test]
    fn test_viewport_snapshot() {
        let mut mux = TerminalMultiplexer::new(
            TerminalSize {
                columns: 80,
                rows: 24,
            },
            2000,
        );

        mux.update_viewport("viewer1", TerminalSize { columns: 80, rows: 24 });

        mux.append("Hello\n");

        let snap = mux.viewport_snapshot("viewer1");
        assert!(snap.is_some());
        let snap = snap.unwrap();
        assert_eq!(snap.view_id, "viewer1");
        assert_eq!(snap.visible_lines.len(), 24);
        assert_eq!(snap.visible_lines[0], "Hello");
    }

    #[test]
    fn test_semantic_change_detection() {
        let mut mux = TerminalMultiplexer::new(
            TerminalSize {
                columns: 80,
                rows: 24,
            },
            2000,
        );

        mux.append("meaningful text\n");
        let change = mux.last_semantic_change();
        assert_eq!(change.kind, TerminalSemanticChangeKind::MeaningfulOutput);
        assert!(change.appended_visible_character_count > 0);
    }

    #[test]
    fn test_snapshot_screen_state() {
        let mut mux = TerminalMultiplexer::new(
            TerminalSize {
                columns: 10,
                rows: 5,
            },
            100,
        );

        mux.append("abc\ndef\n");

        let snap = mux.snapshot();
        assert_eq!(snap.columns, 10);
        assert_eq!(snap.rows, 5);
        assert_eq!(snap.visible_lines[0], "abc");
        assert_eq!(snap.visible_lines[1], "def");
        assert!(snap.render_revision > 0);
    }
}
