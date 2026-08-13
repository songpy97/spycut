use serde::{Deserialize, Serialize};

use crate::domain::{
    export_plan::ExportPlan,
    interval::{DeleteInterval, IntervalError, normalize_intervals, resize_interval},
    project::ProjectV1,
    time::Micros,
};

use super::history::CommandHistory;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionProjection {
    pub project: ProjectV1,
    pub can_undo: bool,
    pub can_redo: bool,
    pub deleted_duration_us: Micros,
    pub kept_duration_us: Micros,
}

#[derive(Clone, Debug)]
pub struct ProjectSession {
    project: ProjectV1,
    history: CommandHistory<ProjectV1>,
}

impl ProjectSession {
    pub fn new(project: ProjectV1) -> Self {
        Self {
            project,
            history: CommandHistory::new(100),
        }
    }

    pub fn project(&self) -> &ProjectV1 {
        &self.project
    }

    pub fn projection(&self) -> Result<SessionProjection, IntervalError> {
        let plan = ExportPlan::build(&self.project.media, &self.project.delete_intervals)?;
        Ok(SessionProjection {
            project: self.project.clone(),
            can_undo: self.history.can_undo(),
            can_redo: self.history.can_redo(),
            deleted_duration_us: plan.deleted_duration_us,
            kept_duration_us: plan.kept_duration_us,
        })
    }

    pub fn add_delete_interval(
        &mut self,
        start_us: Micros,
        end_us: Micros,
    ) -> Result<(), IntervalError> {
        let interval = DeleteInterval::new(self.project.next_interval_id, start_us, end_us)?;
        let mut intervals = self.project.delete_intervals.clone();
        intervals.push(interval);
        let normalized = normalize_intervals(&intervals, self.project.media.duration_us)?;
        self.record_before_edit();
        self.project.delete_intervals = normalized;
        self.project.next_interval_id += 1;
        self.invalidate_reviews();
        Ok(())
    }

    pub fn resize_delete_interval(
        &mut self,
        id: u64,
        start_us: Micros,
        end_us: Micros,
    ) -> Result<(), IntervalError> {
        let intervals = resize_interval(
            &self.project.delete_intervals,
            id,
            start_us,
            end_us,
            self.project.media.duration_us,
        )?;
        self.record_before_edit();
        self.project.delete_intervals = intervals;
        self.invalidate_reviews();
        Ok(())
    }

    pub fn remove_delete_interval(&mut self, id: u64) -> Result<(), IntervalError> {
        if !self
            .project
            .delete_intervals
            .iter()
            .any(|interval| interval.id == id)
        {
            return Err(IntervalError::NotFound(id));
        }
        self.record_before_edit();
        self.project
            .delete_intervals
            .retain(|interval| interval.id != id);
        self.invalidate_reviews();
        Ok(())
    }

    pub fn set_playhead(&mut self, playhead_us: Micros) {
        self.project.last_playhead_us = playhead_us.clamp(0, self.project.media.duration_us);
        self.project.touch();
    }

    pub fn set_reviewed(&mut self, id: u64, reviewed: bool) -> Result<(), IntervalError> {
        if !self
            .project
            .delete_intervals
            .iter()
            .any(|interval| interval.id == id)
        {
            return Err(IntervalError::NotFound(id));
        }
        if reviewed {
            if !self.project.reviewed_interval_ids.contains(&id) {
                self.project.reviewed_interval_ids.push(id);
                self.project.reviewed_interval_ids.sort_unstable();
            }
        } else {
            self.project
                .reviewed_interval_ids
                .retain(|item| *item != id);
        }
        self.project.touch();
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        let Some(project) = self.history.undo(&self.project) else {
            return false;
        };
        self.project = project;
        self.project.touch();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(project) = self.history.redo(&self.project) else {
            return false;
        };
        self.project = project;
        self.project.touch();
        true
    }

    fn record_before_edit(&mut self) {
        self.history.record(&self.project);
    }

    fn invalidate_reviews(&mut self) {
        self.project.reviewed_interval_ids.clear();
        self.project.touch();
    }
}
