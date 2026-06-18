//! Project Tracker — multi-phase project lifecycle management
//!
//! Port of `core/mind/project_tracker.py`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::TdgResult;

/// Project phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectPhase {
    pub name: String,
    pub description: String,
    pub status: String, // "pending", "active", "completed", "deferred"
}

/// A tracked project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub phases: Vec<ProjectPhase>,
    pub current_phase: usize,
    pub created_at: String,
    pub updated_at: String,
    pub status: String, // "active", "completed", "deferred"
    pub metadata: serde_json::Value,
}

/// Project tracker state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectState {
    pub projects: HashMap<String, Project>,
}

/// Default phases for a project.
fn default_phases() -> Vec<ProjectPhase> {
    vec![
        ProjectPhase {
            name: "planning".to_string(),
            description: "Define scope and approach".to_string(),
            status: "pending".to_string(),
        },
        ProjectPhase {
            name: "implementation".to_string(),
            description: "Build the core functionality".to_string(),
            status: "pending".to_string(),
        },
        ProjectPhase {
            name: "testing".to_string(),
            description: "Validate correctness".to_string(),
            status: "pending".to_string(),
        },
        ProjectPhase {
            name: "deployment".to_string(),
            description: "Ship to production".to_string(),
            status: "pending".to_string(),
        },
    ]
}

/// The Project Tracker — manages multi-phase project lifecycles.
pub struct ProjectTracker {
    state: ProjectState,
}

impl ProjectTracker {
    pub fn new() -> Self {
        Self {
            state: ProjectState::default(),
        }
    }

    pub fn with_state(state: ProjectState) -> Self {
        Self { state }
    }

    pub fn state(&self) -> &ProjectState {
        &self.state
    }

    /// Create a new project.
    pub fn create_project(
        &mut self,
        name: &str,
        phases: Option<Vec<ProjectPhase>>,
    ) -> TdgResult<&Project> {
        let id = format!("proj_{}", uuid::Uuid::new_v4().as_simple());
        let now = crate::db::crud::now_iso();

        let project = Project {
            id: id.clone(),
            name: name.to_string(),
            phases: phases.unwrap_or_else(default_phases),
            current_phase: 0,
            created_at: now.clone(),
            updated_at: now,
            status: "active".to_string(),
            metadata: serde_json::json!({}),
        };

        self.state.projects.insert(id.clone(), project);
        Ok(self.state.projects.get(&id).unwrap())
    }

    /// Advance to the next phase.
    pub fn advance_phase(&mut self, project_id: &str) -> TdgResult<&Project> {
        let project = self.state.projects.get_mut(project_id).ok_or_else(|| {
            crate::error::TdgError::Custom(format!("Project {project_id} not found"))
        })?;

        // Mark current phase as completed
        if project.current_phase < project.phases.len() {
            project.phases[project.current_phase].status = "completed".to_string();
        }

        project.current_phase += 1;
        project.updated_at = crate::db::crud::now_iso();

        // Check if all phases complete
        if project.current_phase >= project.phases.len() {
            project.status = "completed".to_string();
        } else {
            project.phases[project.current_phase].status = "active".to_string();
        }

        Ok(self.state.projects.get(project_id).unwrap())
    }

    /// Update the status of the current phase.
    pub fn update_phase_status(&mut self, project_id: &str, status: &str) -> TdgResult<&Project> {
        let project = self.state.projects.get_mut(project_id).ok_or_else(|| {
            crate::error::TdgError::Custom(format!("Project {project_id} not found"))
        })?;

        if project.current_phase < project.phases.len() {
            project.phases[project.current_phase].status = status.to_string();
        }
        project.updated_at = crate::db::crud::now_iso();

        Ok(self.state.projects.get(project_id).unwrap())
    }

    /// Mark a project as deferred.
    pub fn mark_deferred(&mut self, project_id: &str) -> TdgResult<&Project> {
        let project = self.state.projects.get_mut(project_id).ok_or_else(|| {
            crate::error::TdgError::Custom(format!("Project {project_id} not found"))
        })?;

        project.status = "deferred".to_string();
        project.updated_at = crate::db::crud::now_iso();

        Ok(self.state.projects.get(project_id).unwrap())
    }

    /// Get the status of a project.
    pub fn get_status(&self, project_id: &str) -> Option<&Project> {
        self.state.projects.get(project_id)
    }

    /// List all active projects.
    pub fn list_active(&self) -> Vec<&Project> {
        self.state
            .projects
            .values()
            .filter(|p| p.status == "active")
            .collect()
    }

    /// Get a summary for prompt injection.
    pub fn get_summary(&self) -> serde_json::Value {
        let active = self.list_active();
        let total = self.state.projects.len();

        let project_summaries: Vec<serde_json::Value> = active
            .iter()
            .map(|p| {
                let current_phase_name = p
                    .phases
                    .get(p.current_phase)
                    .map(|ph| ph.name.as_str())
                    .unwrap_or("completed");
                serde_json::json!({
                    "id": p.id,
                    "name": p.name,
                    "current_phase": current_phase_name,
                    "progress": format!("{}/{}", p.current_phase, p.phases.len()),
                })
            })
            .collect();

        serde_json::json!({
            "total_projects": total,
            "active_projects": active.len(),
            "projects": project_summaries,
        })
    }
}

impl Default for ProjectTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_advance_project() {
        let mut tracker = ProjectTracker::new();
        let id = tracker
            .create_project("Test Project", None)
            .unwrap()
            .id
            .clone();
        {
            let project = tracker.get_status(&id).unwrap();
            assert_eq!(project.name, "Test Project");
            assert_eq!(project.current_phase, 0);
            assert_eq!(project.status, "active");
        }

        let project = tracker.advance_phase(&id).unwrap();
        assert_eq!(project.current_phase, 1);
        assert_eq!(project.phases[0].status, "completed");
        assert_eq!(project.phases[1].status, "active");
    }

    #[test]
    fn complete_project() {
        let mut tracker = ProjectTracker::new();
        let id = tracker
            .create_project(
                "Short Project",
                Some(vec![ProjectPhase {
                    name: "do".to_string(),
                    description: "Do it".to_string(),
                    status: "pending".to_string(),
                }]),
            )
            .unwrap()
            .id
            .clone();

        let project = tracker.advance_phase(&id).unwrap();
        assert_eq!(project.status, "completed");
    }

    #[test]
    fn list_active() {
        let mut tracker = ProjectTracker::new();
        let _ = tracker.create_project("Active 1", None).unwrap();
        let _ = tracker.create_project("Active 2", None).unwrap();

        let id = tracker.create_project("Deferred", None).unwrap().id.clone();
        let _ = tracker.mark_deferred(&id).unwrap();

        let active = tracker.list_active();
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn summary_output() {
        let mut tracker = ProjectTracker::new();
        let _ = tracker.create_project("Summary Test", None).unwrap();
        let summary = tracker.get_summary();
        assert_eq!(summary["total_projects"], 1);
        assert_eq!(summary["active_projects"], 1);
    }
}
