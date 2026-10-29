//! Project Settings form state (M10 S4c).
//!
//! `project.yaml` remains the canonical manifest, but writers edit it through
//! validated fields instead of raw YAML. The module is UI-framework free: the
//! app renders this state and calls [`ProjectSettingsState::save`], which
//! builds a `Manifest` and delegates the atomic write to
//! `narra_core::project::Project::update_manifest`.

use narra_core::manifest::ProjectType;
use narra_core::project::Project;

/// Which project field owns typing. Type and screenplay are chosen by click;
/// Tab still walks them so the whole form is keyboard-reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectField {
    Title,
    Type,
    PrimaryLanguage,
    AdditionalLanguages,
    DefaultScreenplay,
}

impl ProjectField {
    const ALL: [ProjectField; 5] = [
        ProjectField::Title,
        ProjectField::Type,
        ProjectField::PrimaryLanguage,
        ProjectField::AdditionalLanguages,
        ProjectField::DefaultScreenplay,
    ];

    fn next(self, forward: bool) -> ProjectField {
        let current = Self::ALL
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0);
        let next = if forward {
            (current + 1) % Self::ALL.len()
        } else {
            (current + Self::ALL.len() - 1) % Self::ALL.len()
        };
        Self::ALL[next]
    }
}

#[derive(Debug, Clone)]
pub struct ProjectSettingsState {
    pub title: String,
    pub project_type: ProjectType,
    /// Actual package id; custom ids survive a title/language edit even
    /// though the legacy enum shows `Other`.
    pub project_type_id: String,
    pub primary_language: String,
    /// Editable comma-separated list; normalized on save.
    pub additional_languages: String,
    pub default_screenplay: String,
    pub field: ProjectField,
    pub error: String,
    pub status: String,
}

impl ProjectSettingsState {
    pub fn new(project: &Project) -> Self {
        let manifest = project.manifest();
        Self {
            title: manifest.title.clone(),
            project_type: manifest.project_type,
            project_type_id: manifest.project_type_id.clone(),
            primary_language: manifest.primary_language.clone(),
            additional_languages: manifest.additional_languages.join(", "),
            default_screenplay: manifest.default_screenplay_path.clone(),
            field: ProjectField::Title,
            error: String::new(),
            status: String::new(),
        }
    }

    pub fn input(&mut self, text: &str) {
        let cleaned: String = text
            .chars()
            .filter(|c| !c.is_control() || *c == ' ')
            .collect();
        match self.field {
            ProjectField::Title => self.title.push_str(&cleaned),
            ProjectField::PrimaryLanguage => self.primary_language.push_str(&cleaned),
            ProjectField::AdditionalLanguages => self.additional_languages.push_str(&cleaned),
            ProjectField::Type | ProjectField::DefaultScreenplay => {}
        }
        self.error.clear();
        self.status.clear();
    }

    pub fn backspace(&mut self) {
        let target = match self.field {
            ProjectField::Title => &mut self.title,
            ProjectField::PrimaryLanguage => &mut self.primary_language,
            ProjectField::AdditionalLanguages => &mut self.additional_languages,
            ProjectField::Type | ProjectField::DefaultScreenplay => return,
        };
        if let Some(last) = target.chars().last() {
            let len = target.len();
            target.truncate(len - last.len_utf8());
        }
        self.error.clear();
        self.status.clear();
    }

    pub fn cycle_field(&mut self, forward: bool) {
        self.field = self.field.next(forward);
    }

    /// Build from the opened project (preserving id and creation time) and
    /// write through the core. On success the in-memory project is updated.
    pub fn save(&self, project: &mut Project) -> Result<(), String> {
        let mut manifest = project.manifest().clone();
        manifest.title = self.title.trim().to_string();
        manifest.project_type = self.project_type;
        let keep_existing_id = ProjectType::parse(&self.project_type_id)
            .map_or(self.project_type == ProjectType::Other, |known| {
                known == self.project_type
            });
        manifest.project_type_id = if keep_existing_id {
            self.project_type_id.clone()
        } else {
            self.project_type.as_str().to_string()
        };
        manifest.primary_language = self.primary_language.trim().to_string();
        manifest.additional_languages = self
            .additional_languages
            .split(',')
            .map(str::trim)
            .filter(|language| !language.is_empty())
            .map(str::to_string)
            .collect();
        manifest.default_screenplay_path = self.default_screenplay.trim().to_string();
        project
            .update_manifest(manifest)
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        let root = std::env::temp_dir().join(format!(
            "narra-project-settings-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp dir");
        narra_core::project::create_project(&root, "Original", ProjectType::Feature, "es")
            .expect("create")
    }

    #[test]
    fn form_loads_current_manifest() {
        let project = project();
        let state = ProjectSettingsState::new(&project);
        assert_eq!(state.title, "Original");
        assert_eq!(state.project_type, ProjectType::Feature);
        assert_eq!(state.primary_language, "es");
        assert_eq!(
            state.default_screenplay,
            narra_core::project::DEFAULT_SCREENPLAY
        );
        assert_eq!(state.field, ProjectField::Title);
    }

    #[test]
    fn custom_project_type_id_survives_a_form_save() {
        let root = std::env::temp_dir().join(format!(
            "narra-project-settings-custom-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let package = narra_core::project_type::parse_package(
            "schema_version: 1\napi_version: 1\nid: documentary\nname: Documentary\n",
            narra_core::project_type::TypeOrigin::User,
        )
        .expect("package");
        let mut project =
            narra_core::project::create_project_from_package(&root, "Documental", &package, "es")
                .expect("create");
        let mut state = ProjectSettingsState::new(&project);
        state.title = "Documental nuevo".to_string();
        state.save(&mut project).expect("save");
        assert_eq!(project.manifest().title, "Documental nuevo");
        assert_eq!(project.manifest().project_type_id, "documentary");
    }

    #[test]
    fn save_updates_manifest_and_preserves_identity() {
        let mut project = project();
        let id = project.manifest().project_id;
        let created = project.manifest().created_at.clone();
        let mut state = ProjectSettingsState::new(&project);
        state.title = "  Nueva serie  ".to_string();
        state.project_type = ProjectType::Series;
        state.primary_language = "en".to_string();
        state.additional_languages = "es, fr".to_string();
        state.save(&mut project).expect("save");
        assert_eq!(project.manifest().title, "Nueva serie");
        assert_eq!(project.manifest().project_type, ProjectType::Series);
        assert_eq!(project.manifest().primary_language, "en");
        assert_eq!(project.manifest().additional_languages, vec!["es", "fr"]);
        assert_eq!(project.manifest().project_id, id);
        assert_eq!(project.manifest().created_at, created);
        let reopened = narra_core::project::open_project(project.root()).expect("reopen");
        assert_eq!(reopened.manifest(), project.manifest());
    }

    #[test]
    fn invalid_values_are_rejected_without_touching_the_file() {
        let mut project = project();
        let mut state = ProjectSettingsState::new(&project);
        state.title = "   ".to_string();
        assert!(state.save(&mut project).is_err());
        assert_eq!(project.manifest().title, "Original");
        let reopened = narra_core::project::open_project(project.root()).expect("reopen");
        assert_eq!(reopened.manifest().title, "Original");
    }

    #[test]
    fn field_cycle_wraps_and_backspace_is_grapheme_safe() {
        let mut state = ProjectSettingsState::new(&project());
        state.field = ProjectField::Title;
        state.title = "Marta".to_string();
        state.cycle_field(false);
        assert_eq!(state.field, ProjectField::DefaultScreenplay);
        state.cycle_field(true);
        assert_eq!(state.field, ProjectField::Title);
        state.backspace();
        assert_eq!(state.title, "Mart");
    }
}
