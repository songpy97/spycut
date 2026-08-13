use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct CommandHistory<T> {
    past: VecDeque<T>,
    future: Vec<T>,
    limit: usize,
}

impl<T: Clone> CommandHistory<T> {
    pub fn new(limit: usize) -> Self {
        Self {
            past: VecDeque::with_capacity(limit),
            future: Vec::new(),
            limit,
        }
    }

    pub fn record(&mut self, current: &T) {
        if self.limit == 0 {
            return;
        }
        if self.past.len() == self.limit {
            self.past.pop_front();
        }
        self.past.push_back(current.clone());
        self.future.clear();
    }

    pub fn undo(&mut self, current: &T) -> Option<T> {
        let previous = self.past.pop_back()?;
        self.future.push(current.clone());
        Some(previous)
    }

    pub fn redo(&mut self, current: &T) -> Option<T> {
        let next = self.future.pop()?;
        if self.limit > 0 {
            if self.past.len() == self.limit {
                self.past.pop_front();
            }
            self.past.push_back(current.clone());
        }
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
}
