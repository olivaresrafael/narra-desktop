//! Bible entity index and mention matcher (M11 slices 2–3, D-029).
//!
//! Canonical entities are ordinary Markdown files under
//! `bible/characters/`, `bible/locations/` and `bible/objects/`; the file
//! stem is the display name. This module only derives an index from
//! `Project::list_documents` — it never writes, never scans outside the
//! project and never treats the derived index as canon.
//!
//! Matching rules (D-029):
//! - case-insensitive;
//! - word boundaries (a match cannot be a fragment of a larger word);
//! - longest name first, so `Marta Ruiz` wins over `Marta`;
//! - accents are preserved (`DÍA` matches `día`, never `dia`);
//! - no fuzzy matching, NER or model extraction.

use std::collections::HashSet;

use crate::project::Project;

/// Bible category of an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityCategory {
    Character,
    Location,
    Object,
}

impl EntityCategory {
    pub const ALL: [EntityCategory; 3] = [
        EntityCategory::Character,
        EntityCategory::Location,
        EntityCategory::Object,
    ];

    pub fn key(self) -> &'static str {
        match self {
            EntityCategory::Character => "character",
            EntityCategory::Location => "location",
            EntityCategory::Object => "object",
        }
    }

    pub fn folder(self) -> &'static str {
        match self {
            EntityCategory::Character => "bible/characters",
            EntityCategory::Location => "bible/locations",
            EntityCategory::Object => "bible/objects",
        }
    }

    pub fn from_path(path: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|category| {
            path.starts_with(category.folder())
                && path.len() > category.folder().len() + 1
                && path.ends_with(".md")
        })
    }
}

/// One derived entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entity {
    /// Display name (the file stem, case preserved).
    pub name: String,
    pub category: EntityCategory,
    /// Project-relative Markdown path.
    pub document: String,
}

/// One mention range within a line (byte offsets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    pub start: usize,
    pub end: usize,
    /// Index into [`EntityIndex::entities`].
    pub entity: usize,
}

impl Mention {
    pub fn category(&self, index: &EntityIndex) -> EntityCategory {
        index.entities[self.entity].category
    }

    pub fn document<'a>(&self, index: &'a EntityIndex) -> &'a str {
        &index.entities[self.entity].document
    }

    pub fn name<'a>(&self, index: &'a EntityIndex) -> &'a str {
        &index.entities[self.entity].name
    }
}

/// Derived, disposable entity index.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EntityIndex {
    entities: Vec<Entity>,
}

impl EntityIndex {
    /// Build the index from the project's bible folders. Files outside those
    /// three folders are ignored; unreadable folders are reported by
    /// `list_documents` and never guessed around.
    pub fn build(project: &Project) -> Result<Self, crate::project::ProjectError> {
        let mut entities = Vec::new();
        for document in project.list_documents()? {
            let Some(category) = EntityCategory::from_path(&document) else {
                continue;
            };
            let Some(stem) = document
                .rsplit('/')
                .next()
                .and_then(|name| name.strip_suffix(".md"))
            else {
                continue;
            };
            if stem.trim().is_empty() {
                continue;
            }
            entities.push(Entity {
                name: stem.to_string(),
                category,
                document,
            });
        }
        // Path order is deterministic, so equal names resolve consistently.
        entities.sort_by(|a, b| a.document.cmp(&b.document));
        Ok(Self { entities })
    }

    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Find a dossier by display name (case-insensitive). Used by the
    /// context menu before creating a new entity.
    pub fn find_by_name(&self, name: &str) -> Option<&Entity> {
        let needle = name.trim().to_lowercase();
        self.entities
            .iter()
            .find(|entity| entity.name.to_lowercase() == needle)
    }

    /// All mentions in one line, in ascending byte order and non-overlapping.
    ///
    /// Names are tried longest-first so a multi-word entity wins over a
    /// shorter one starting at the same place; overlaps are skipped instead
    /// of rendered twice.
    pub fn mentions(&self, line: &str) -> Vec<Mention> {
        if line.is_empty() || self.entities.is_empty() {
            return Vec::new();
        }
        let lower = line.to_lowercase();
        let mut order: Vec<usize> = (0..self.entities.len()).collect();
        order.sort_by(|a, b| {
            self.entities[*b]
                .name
                .chars()
                .count()
                .cmp(&self.entities[*a].name.chars().count())
                .then_with(|| self.entities[*a].document.cmp(&self.entities[*b].document))
        });
        let mut occupied = vec![false; line.len()];
        let mut found = Vec::new();
        for index in order {
            let needle = self.entities[index].name.to_lowercase();
            if needle.is_empty() {
                continue;
            }
            let mut from = 0;
            while let Some(relative) = lower[from..].find(&needle) {
                let start = from + relative;
                let end = start + needle.len();
                if is_word_boundary(line, start, end)
                    && occupied[start..end].iter().all(|taken| !taken)
                {
                    for slot in &mut occupied[start..end] {
                        *slot = true;
                    }
                    found.push(Mention {
                        start,
                        end,
                        entity: index,
                    });
                }
                from = end.max(start + 1);
                if from >= lower.len() {
                    break;
                }
            }
        }
        found.sort_by_key(|mention| (mention.start, mention.end));
        found
    }

    /// A stable, bounded diagnostic snapshot for the dump.
    pub fn dump_json(&self, limit: usize) -> String {
        self.entities
            .iter()
            .take(limit)
            .map(|entity| {
                format!(
                    "{{\"name\":\"{}\",\"category\":\"{}\",\"document\":\"{}\"}}",
                    json_escape(&entity.name),
                    entity.category.key(),
                    json_escape(&entity.document)
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\'' || c == '’'
}

fn is_word_boundary(line: &str, start: usize, end: usize) -> bool {
    let before = line[..start].chars().next_back();
    let after = line[end..].chars().next();
    before.is_none_or(|c| !is_word_char(c)) && after.is_none_or(|c| !is_word_char(c))
}

fn json_escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Divides a line into non-overlapping runs by entity mentions. Returns
/// `(range, Option<entity index>)`, covering the whole line in order. Used by
/// the renderer to build styled runs without duplicating matcher logic.
pub fn split_by_mentions(
    line: &str,
    mentions: &[Mention],
) -> Vec<(std::ops::Range<usize>, Option<usize>)> {
    let mut runs = Vec::new();
    let mut cursor = 0;
    for mention in mentions {
        if mention.start > cursor {
            runs.push((cursor..mention.start, None));
        }
        if mention.end > mention.start {
            runs.push((mention.start..mention.end, Some(mention.entity)));
        }
        cursor = mention.end.max(cursor);
    }
    if cursor < line.len() {
        runs.push((cursor..line.len(), None));
    }
    if runs.is_empty() {
        runs.push((0..line.len(), None));
    }
    runs
}

/// Set of entity names currently indexed. Useful for the context menu to
/// decide between “open dossier” and “create dossier”.
pub fn names(index: &EntityIndex) -> HashSet<String> {
    index
        .entities
        .iter()
        .map(|entity| entity.name.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ProjectType;
    use crate::project::{create_project, open_project};
    use std::fs;

    fn project() -> crate::project::Project {
        let root = std::env::temp_dir().join(format!(
            "narra-entities-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("temp dir");
        let project =
            create_project(&root, "Entidades", ProjectType::Feature, "es").expect("create");
        for (path, body) in [
            ("bible/characters/Marta.md", "# Marta\n"),
            ("bible/characters/Marta Ruiz.md", "# Marta Ruiz\n"),
            ("bible/locations/Casa de Marta.md", "# Casa\n"),
            ("bible/objects/reloj de Marta.md", "# Reloj\n"),
            ("notes/not-an-entity.md", "# Nota\n"),
        ] {
            project.create_document(path, body).expect("document");
        }
        open_project(&root).expect("reopen")
    }

    #[test]
    fn index_reads_only_the_three_bible_folders() {
        let index = EntityIndex::build(&project()).expect("index");
        assert_eq!(index.len(), 4);
        let names: Vec<&str> = index.entities().iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Marta"));
        assert!(names.contains(&"Marta Ruiz"));
        assert!(names.contains(&"Casa de Marta"));
        assert!(names.contains(&"reloj de Marta"));
        assert!(!names.contains(&"not-an-entity"));
        assert_eq!(index.entities()[0].category, EntityCategory::Character);
    }

    #[test]
    fn mentions_are_case_insensitive_accent_preserving_and_word_bounded() {
        let index = EntityIndex::build(&project()).expect("index");
        let mentions = index.mentions("MARTA entra; mártires no. DÍA en la Casa de Marta.");
        let names: Vec<&str> = mentions.iter().map(|m| m.name(&index)).collect();
        assert_eq!(names, vec!["Marta", "Casa de Marta"]);
        // A substring inside another word is not a mention.
        let none = index.mentions("mártires y casamiento");
        assert!(none.is_empty(), "{none:?}");
        // Accents stay significant: the entity is “Marta”, not “Márta”.
        assert!(index.mentions("Márta").is_empty());
    }

    #[test]
    fn longest_name_wins_and_overlaps_are_not_duplicated() {
        let index = EntityIndex::build(&project()).expect("index");
        let mentions = index.mentions("Marta Ruiz llega; Marta espera.");
        assert_eq!(mentions.len(), 2);
        assert_eq!(mentions[0].name(&index), "Marta Ruiz");
        assert_eq!(mentions[1].name(&index), "Marta");
        assert!(mentions[0].end <= mentions[1].start);
    }

    #[test]
    fn split_runs_cover_the_line_once() {
        let index = EntityIndex::build(&project()).expect("index");
        let line = "Marta entra en la Casa de Marta.";
        let mentions = index.mentions(line);
        let runs = split_by_mentions(line, &mentions);
        let rebuilt: String = runs.iter().map(|(range, _)| &line[range.clone()]).collect();
        assert_eq!(rebuilt, line);
        assert!(runs.iter().any(|(_, entity)| entity.is_some()));
    }

    #[test]
    fn find_by_name_is_case_insensitive() {
        let index = EntityIndex::build(&project()).expect("index");
        assert_eq!(
            index
                .find_by_name("marta")
                .map(|entity| entity.document.as_str()),
            Some("bible/characters/Marta.md")
        );
        assert!(index.find_by_name("nadie").is_none());
    }
}
