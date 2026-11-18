use super::*;

/// Find-in-page state: query, hits and the current hit index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FindState {
    query: String,
    matches: Vec<FindMatch>,
    current: usize,
}

impl FindState {
    /// Creates an empty, closed find state.
    pub const fn new() -> Self {
        Self {
            query: String::new(),
            matches: Vec::new(),
            current: 0,
        }
    }

    /// Current query (empty when closed).
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Hits of the current query.
    pub fn matches(&self) -> &[FindMatch] {
        &self.matches
    }

    /// Number of hits.
    pub fn len(&self) -> usize {
        self.matches.len()
    }

    /// Whether there are no hits.
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }

    /// Index of the current hit (`0` when there are no hits).
    pub const fn current(&self) -> usize {
        self.current
    }

    /// Current hit, if any.
    pub fn current_match(&self) -> Option<&FindMatch> {
        self.matches.get(self.current)
    }

    /// Replaces the query/hits (used after a render with the new query); resets to hit 0.
    pub fn set_results(&mut self, query: impl Into<String>, matches: Vec<FindMatch>) {
        self.query = query.into();
        self.matches = matches;
        self.current = 0;
    }

    /// Replaces the hit list without resetting the current index (clamped to the new list).
    pub fn set_matches(&mut self, matches: Vec<FindMatch>) {
        self.matches = matches;
        if self.current >= self.matches.len() {
            self.current = 0;
        }
    }

    /// Advances to the next hit, wrapping around to the first one.
    pub fn next_hit(&mut self) -> bool {
        if self.matches.is_empty() {
            self.current = 0;
            return false;
        }
        self.current = (self.current + 1) % self.matches.len();
        true
    }

    /// Goes back to the previous hit, wrapping around to the last one.
    pub fn previous_hit(&mut self) -> bool {
        if self.matches.is_empty() {
            self.current = 0;
            return false;
        }
        self.current = if self.current == 0 {
            self.matches.len() - 1
        } else {
            self.current - 1
        };
        true
    }

    /// Selects `index` (clamped to the hit list).
    pub fn set_current(&mut self, index: usize) {
        self.current = index.min(self.matches.len().saturating_sub(1));
    }

    /// Closes the search (clears query and hits).
    pub fn clear(&mut self) {
        self.query.clear();
        self.matches.clear();
        self.current = 0;
    }

    /// Highlight rectangles of the current query, recomputed from `layout`.
    ///
    /// The current hit is index-clamped modulo the rectangle count (the DOM hit list and the
    /// layout fragment list can differ when one fragment contains several hits).
    pub fn current_rect(&self, layout: &LayoutBox) -> Option<CssRect> {
        if self.query.is_empty() {
            return None;
        }
        let rects = highlight_rects(layout, &self.query);
        if rects.is_empty() {
            return None;
        }
        Some(rects[self.current % rects.len()])
    }
}
