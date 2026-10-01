pub mod inspector;
pub mod port_view;
pub mod process_view;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort<C> {
    pub column: C,
    pub ascending: bool,
}

/// Cursor and scroll window for a table whose rows we slice ourselves. Owning the
/// offset (instead of letting ratatui derive it from the selection) is what lets the
/// mouse wheel scroll without moving the selection.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TableCursor {
    pub cursor: usize,
    pub offset: usize,
    /// Rows that fit on screen, as of the last render.
    pub viewport: usize,
    /// Whether the window should keep the cursor visible. Wheel scrolling turns this
    /// off so a refresh does not yank the view back to the selection.
    pub follow: bool,
}

impl TableCursor {
    pub fn move_by(&mut self, len: usize, delta: i64) {
        if len == 0 {
            return;
        }
        let target = (self.cursor as i64 + delta).clamp(0, len as i64 - 1);
        self.select(len, target as usize);
    }

    pub fn select(&mut self, len: usize, index: usize) {
        if len == 0 {
            return;
        }
        self.cursor = index.min(len - 1);
        self.follow = true;
        self.ensure_visible();
    }

    /// Called after the row list changes length or the viewport is re-measured.
    pub fn fit(&mut self, len: usize, viewport: usize) {
        self.viewport = viewport;
        self.cursor = self.cursor.min(len.saturating_sub(1));
        self.offset = self.offset.min(len.saturating_sub(viewport));
        if self.follow {
            self.ensure_visible();
        }
    }

    fn ensure_visible(&mut self) {
        if self.viewport == 0 {
            return;
        }
        if self.cursor < self.offset {
            self.offset = self.cursor;
        } else if self.cursor >= self.offset + self.viewport {
            self.offset = self.cursor + 1 - self.viewport;
        }
    }

    /// Position of the cursor within the visible window, if it is on screen.
    pub fn visible_cursor(&self) -> Option<usize> {
        (self.cursor >= self.offset && self.cursor < self.offset + self.viewport)
            .then(|| self.cursor - self.offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor(viewport: usize) -> TableCursor {
        TableCursor {
            viewport,
            ..Default::default()
        }
    }

    #[test]
    fn moving_past_window_scrolls_it() {
        let mut c = cursor(5);
        c.move_by(100, 7);
        assert_eq!((c.cursor, c.offset), (7, 3));
        c.move_by(100, -6);
        assert_eq!((c.cursor, c.offset), (1, 1));
    }

    #[test]
    fn moves_clamp_to_list() {
        let mut c = cursor(5);
        c.move_by(10, 50);
        assert_eq!(c.cursor, 9);
        c.move_by(10, -50);
        assert_eq!(c.cursor, 0);
        c.move_by(0, 1);
        assert_eq!(c.cursor, 0);
    }

    #[test]
    fn fit_clamps_after_list_shrinks() {
        let mut c = cursor(5);
        c.select(100, 50);
        c.fit(20, 5);
        assert_eq!((c.cursor, c.offset), (19, 15));
        assert_eq!(c.visible_cursor(), Some(4));
    }
}
