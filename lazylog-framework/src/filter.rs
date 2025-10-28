use crate::provider::{LogDetailLevel, LogItem, LogItemFormatter};
use rayon::prelude::*;
use std::sync::Arc;

/// filtering engine with incremental filtering and parallel processing
pub struct FilterEngine {
    /// previous filter query for incremental filtering
    previous_query: String,
    /// cached results from previous filter
    previous_results: Vec<usize>,
    /// formatter for converting log items to searchable text
    formatter: Option<Arc<dyn LogItemFormatter>>,
}

impl FilterEngine {
    /// create a new filter engine
    pub fn new() -> Self {
        Self {
            previous_query: String::new(),
            previous_results: Vec::new(),
            formatter: None,
        }
    }

    /// set the formatter to use for filtering
    pub fn set_formatter(&mut self, formatter: Arc<dyn LogItemFormatter>) {
        self.formatter = Some(formatter);
    }

    /// filter logs and return indices of matching items
    ///
    /// uses incremental filtering when possible (query extends previous query)
    /// uses parallel processing for large search spaces
    pub fn filter(
        &mut self,
        raw_logs: &[LogItem],
        query: &str,
        detail_level: LogDetailLevel,
    ) -> Vec<usize> {
        // empty query = show all
        if query.is_empty() {
            self.reset();
            return (0..raw_logs.len()).collect();
        }

        // if no formatter is set, return all items
        let Some(formatter) = &self.formatter else {
            return (0..raw_logs.len()).collect();
        };

        // check if we can use incremental filtering
        let can_use_incremental = !self.previous_query.is_empty()
            && query.starts_with(&self.previous_query)
            && !self.previous_results.is_empty();

        let search_space: Vec<usize> = if can_use_incremental {
            // search only within previous results
            self.previous_results.clone()
        } else {
            // full search
            (0..raw_logs.len()).collect()
        };

        // pre-lowercase the pattern once (not 50K times!)
        let pattern_lower = query.to_lowercase();

        // use parallel filtering for large search spaces
        let filtered_indices = if search_space.len() > 1000 {
            self.filter_parallel(
                raw_logs,
                &search_space,
                &pattern_lower,
                detail_level,
                formatter,
            )
        } else {
            self.filter_sequential(
                raw_logs,
                &search_space,
                &pattern_lower,
                detail_level,
                formatter,
            )
        };

        // cache for next filter
        self.previous_query = query.to_string();
        self.previous_results = filtered_indices.clone();

        filtered_indices
    }

    /// reset the filter cache
    pub fn reset(&mut self) {
        self.previous_query.clear();
        self.previous_results.clear();
    }

    /// filter only newly added logs and append to existing results
    ///
    /// this is more efficient than re-filtering all logs when new items arrive
    pub fn filter_new_logs(
        &mut self,
        raw_logs: &[LogItem],
        old_count: usize,
        query: &str,
        detail_level: LogDetailLevel,
    ) -> Vec<usize> {
        // if query changed or no previous results, do full filter
        if query != self.previous_query {
            return self.filter(raw_logs, query, detail_level);
        }

        // if no new logs, return cached results
        if old_count >= raw_logs.len() {
            return self.previous_results.clone();
        }

        // empty query = show all (including new ones)
        if query.is_empty() {
            return (0..raw_logs.len()).collect();
        }

        // if no formatter is set, return all items
        let Some(formatter) = &self.formatter else {
            return (0..raw_logs.len()).collect();
        };

        // filter only the new logs
        let new_indices: Vec<usize> = (old_count..raw_logs.len()).collect();
        let pattern_lower = query.to_lowercase();

        let new_filtered = if new_indices.len() > 1000 {
            self.filter_parallel(
                raw_logs,
                &new_indices,
                &pattern_lower,
                detail_level,
                formatter,
            )
        } else {
            self.filter_sequential(
                raw_logs,
                &new_indices,
                &pattern_lower,
                detail_level,
                formatter,
            )
        };

        // append new filtered indices to existing results
        let mut all_results = self.previous_results.clone();
        all_results.extend(new_filtered);

        // update cache
        self.previous_results = all_results.clone();
        // query stays the same, so previous_query doesn't need updating

        all_results
    }

    /// sequential filtering (for small search spaces)
    fn filter_sequential(
        &self,
        raw_logs: &[LogItem],
        search_space: &[usize],
        pattern_lower: &str,
        detail_level: LogDetailLevel,
        formatter: &Arc<dyn LogItemFormatter>,
    ) -> Vec<usize> {
        search_space
            .iter()
            .filter(|&&idx| {
                let item = &raw_logs[idx];
                formatter
                    .get_searchable_text(item, detail_level)
                    .to_lowercase()
                    .contains(pattern_lower)
            })
            .copied()
            .collect()
    }

    /// parallel filtering (for large search spaces)
    fn filter_parallel(
        &self,
        raw_logs: &[LogItem],
        search_space: &[usize],
        pattern_lower: &str,
        detail_level: LogDetailLevel,
        formatter: &Arc<dyn LogItemFormatter>,
    ) -> Vec<usize> {
        search_space
            .par_iter()
            .filter(|&&idx| {
                let item = &raw_logs[idx];
                formatter
                    .get_searchable_text(item, detail_level)
                    .to_lowercase()
                    .contains(pattern_lower)
            })
            .copied()
            .collect()
    }
}

impl Default for FilterEngine {
    fn default() -> Self {
        Self::new()
    }
}
