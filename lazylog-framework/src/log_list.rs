use ratatui::widgets::ListState;

pub struct LogList {
    pub indices: Vec<usize>,
    pub state: ListState,
}

impl LogList {
    pub fn new(indices: Vec<usize>) -> Self {
        Self {
            indices,
            state: ListState::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.indices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn get(&self, idx: usize) -> Option<usize> {
        self.indices.get(idx).copied()
    }

    pub fn select_next_circular(&mut self) {
        let len = self.indices.len();
        if len == 0 {
            self.state.select(None);
            return;
        }

        let current = self.state.selected();
        let next = match current {
            Some(i) => {
                if i + 1 >= len {
                    0 // Wrap to first item
                } else {
                    i.saturating_add(1)
                }
            }
            None => 0, // Select first item if nothing is selected
        };
        self.state.select(Some(next));
    }

    pub fn select_previous_circular(&mut self) {
        let len = self.indices.len();
        if len == 0 {
            self.state.select(None);
            return;
        }

        let current = self.state.selected();
        let prev = match current {
            Some(i) => {
                if i == 0 {
                    len - 1 // Wrap to last item
                } else {
                    i.saturating_sub(1)
                }
            }
            None => len - 1, // Select last item if nothing is selected
        };
        self.state.select(Some(prev));
    }

    pub fn select_next(&mut self) {
        let len = self.indices.len();
        if len == 0 {
            self.state.select(None);
            return;
        }

        let current = self.state.selected();
        let next = match current {
            Some(i) => {
                if i + 1 >= len {
                    len - 1 // Stay at last item, no wrap
                } else {
                    i.saturating_add(1)
                }
            }
            None => 0, // Select first item if nothing is selected
        };
        self.state.select(Some(next));
    }

    pub fn select_previous(&mut self) {
        let len = self.indices.len();
        if len == 0 {
            self.state.select(None);
            return;
        }

        let current = self.state.selected();
        let prev = match current {
            Some(i) => {
                if i == 0 {
                    0 // Stay at first item, no wrap
                } else {
                    i.saturating_sub(1)
                }
            }
            None => 0, // Select first item if nothing is selected
        };
        self.state.select(Some(prev));
    }

    pub fn select_first(&mut self) {
        if self.indices.is_empty() {
            self.state.select(None);
        } else {
            self.state.select(Some(0));
        }
    }
}
