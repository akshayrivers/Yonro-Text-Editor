//! Lore book (`PLAN.md Phase 4.5`): named world entities (characters,
//! places, factions, items) plus `@mention` parsing.
//!
//! Entities resolve by name *or alias*, longest match wins — so a registered
//! `"Mara Stone"` parses as one mention, not `@Mara` plus prose. Unknown
//! `@tokens` still parse (as unresolved) so the UI can offer to create them.
//! Ids are arena indices and never reused (tombstones), mirroring the
//! manuscript module.

use std::fmt;

use super::manuscript::Manuscript;

pub type EntityId = usize;

/// What kind of world element an entity is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EntityKind {
    Character,
    Place,
    Faction,
    Item,
    Lore,
}

impl EntityKind {
    /// Parse a frontend `kind` string (`"character"`, `"place"`,
    /// `"faction"`, `"item"`, `"lore"`; case-insensitive, trimmed).
    #[must_use]
    pub fn parse(kind: &str) -> Option<Self> {
        match kind.trim().to_lowercase().as_str() {
            "character" => Some(Self::Character),
            "place" => Some(Self::Place),
            "faction" => Some(Self::Faction),
            "item" => Some(Self::Item),
            "lore" => Some(Self::Lore),
            _ => None,
        }
    }
}

/// One named world element with an optional lore sheet.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub kind: EntityKind,
    pub name: String,
    pub aliases: Vec<String>,
    /// Free-form lore sheet (appearance, history, secrets…).
    pub sheet: String,
    /// `false` after removal (tombstone keeps ids stable).
    pub alive: bool,
}

/// Registry failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoreError {
    UnknownEntity(EntityId),
    InactiveEntity(EntityId),
    EmptyName,
    DuplicateName(String),
    /// `kind` was not a known entity kind.
    BadKind(String),
}

impl fmt::Display for LoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownEntity(id) => write!(formatter, "unknown lore entity {id}"),
            Self::InactiveEntity(id) => write!(formatter, "removed lore entity {id}"),
            Self::EmptyName => write!(formatter, "lore entity names cannot be empty"),
            Self::DuplicateName(name) => {
                write!(formatter, "lore entity {name:?} already exists")
            }
            Self::BadKind(kind) => write!(
                formatter,
                "unknown entity kind {kind:?}: expected character, place, faction, item, or lore"
            ),
        }
    }
}

impl std::error::Error for LoreError {}

/// Human-readable error for the GUI (`DuplicateName` names the clash:
/// `"name already in use: Mara"`).
#[must_use]
pub fn lore_error_message(err: &LoreError) -> String {
    match err {
        LoreError::DuplicateName(name) => format!("name already in use: {name}"),
        LoreError::EmptyName => "name cannot be empty".to_string(),
        LoreError::UnknownEntity(id) => format!("unknown lore entity {id}"),
        LoreError::InactiveEntity(id) => format!("removed lore entity {id}"),
        LoreError::BadKind(kind) => {
            format!("unknown kind {kind:?}: expected character, place, faction, item, or lore")
        }
    }
}

/// An `@mention` span inside draft text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    /// Byte range of the mention *including* the `@`, into the source text.
    pub byte_range: std::ops::Range<usize>,
    /// The matched name exactly as registered (or the raw token).
    pub name: String,
    /// Resolved entity, if the name is known.
    pub entity: Option<EntityId>,
}

/// The world bible: every character, place, faction, item worth `@`-ing.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LoreBook {
    entities: Vec<Entity>,
}

impl LoreBook {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Live entity lookup (`None` for unknown or removed ids).
    #[must_use]
    pub fn get(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(id).filter(|entity| entity.alive)
    }

    /// Case-insensitive name equality helper.
    fn same_name(a: &str, b: &str) -> bool {
        a.eq_ignore_ascii_case(b)
    }

    /// All names (primary + aliases) currently claimed, lowercased.
    fn claimed(&self, except: Option<EntityId>) -> Vec<String> {
        self.entities
            .iter()
            .filter(|entity| entity.alive && Some(entity.id) != except)
            .flat_map(|entity| {
                std::iter::once(&entity.name)
                    .chain(entity.aliases.iter())
                    .map(|name| name.to_lowercase())
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Register an entity. Names (and later aliases) must be unique
    /// case-insensitively so mentions resolve unambiguously.
    ///
    /// # Errors
    /// `EmptyName` for blank names, `DuplicateName` on collision.
    pub fn add(&mut self, kind: EntityKind, name: &str) -> Result<EntityId, LoreError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(LoreError::EmptyName);
        }
        if self
            .claimed(None)
            .iter()
            .any(|taken| taken == &name.to_lowercase())
        {
            return Err(LoreError::DuplicateName(name.to_string()));
        }
        let id = self.entities.len();
        self.entities.push(Entity {
            id,
            kind,
            name: name.to_string(),
            aliases: Vec::new(),
            sheet: String::new(),
            alive: true,
        });
        Ok(id)
    }

    /// Add an alias (alternate spelling, nickname, title).
    ///
    /// # Errors
    /// `UnknownEntity`/`InactiveEntity`, `EmptyName`, `DuplicateName`.
    pub fn add_alias(&mut self, id: EntityId, alias: &str) -> Result<(), LoreError> {
        let alias = alias.trim();
        if alias.is_empty() {
            return Err(LoreError::EmptyName);
        }
        if self
            .claimed(Some(id))
            .iter()
            .any(|taken| taken == &alias.to_lowercase())
        {
            return Err(LoreError::DuplicateName(alias.to_string()));
        }
        match self.entities.get_mut(id) {
            Some(entity) if entity.alive => {
                entity.aliases.push(alias.to_string());
                Ok(())
            }
            Some(_) => Err(LoreError::InactiveEntity(id)),
            None => Err(LoreError::UnknownEntity(id)),
        }
    }

    /// Rename an entity (shorthand for `update` with only a name).
    ///
    /// # Errors
    /// `UnknownEntity`/`InactiveEntity`, `EmptyName`, `DuplicateName`.
    pub fn rename(&mut self, id: EntityId, name: &str) -> Result<(), LoreError> {
        self.update(id, Some(name), None, None)
    }

    /// Replace an entity's alias list wholesale (shorthand for `update`).
    ///
    /// # Errors
    /// `UnknownEntity`/`InactiveEntity`, `EmptyName`, `DuplicateName`.
    pub fn set_aliases(&mut self, id: EntityId, aliases: &[String]) -> Result<(), LoreError> {
        self.update(id, None, Some(aliases), None)
    }

    /// Update an entity's name, aliases, and/or sheet in one atomic step:
    /// every `Some(...)` field is validated first, so a failure changes
    /// nothing. `None` leaves that field untouched; `Some(list)` replaces
    /// the whole alias list.
    ///
    /// # Errors
    /// `UnknownEntity`/`InactiveEntity`, `EmptyName` for blank names or
    /// aliases, `DuplicateName` on any collision (case-insensitive,
    /// including an alias matching the entity's own new name).
    pub fn update(
        &mut self,
        id: EntityId,
        name: Option<&str>,
        aliases: Option<&[String]>,
        sheet: Option<&str>,
    ) -> Result<(), LoreError> {
        let current = match self.entities.get(id) {
            Some(entity) if entity.alive => entity.clone(),
            Some(_) => return Err(LoreError::InactiveEntity(id)),
            None => return Err(LoreError::UnknownEntity(id)),
        };
        // Validate the new name (if any) against every other live entity.
        let next_name: String = match name {
            Some(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    return Err(LoreError::EmptyName);
                }
                if self
                    .claimed(Some(id))
                    .iter()
                    .any(|taken| taken == &trimmed.to_lowercase())
                {
                    return Err(LoreError::DuplicateName(trimmed.to_string()));
                }
                trimmed.to_string()
            }
            None => current.name.clone(),
        };
        // Validate the new alias list (if any): trimmed, non-empty,
        // unique within itself, and colliding with nothing else —
        // including the entity's own (possibly new) name.
        let next_aliases: Vec<String> = match aliases {
            Some(list) => {
                let mut clean: Vec<String> = Vec::new();
                for raw in list {
                    let trimmed = raw.trim();
                    if trimmed.is_empty() {
                        return Err(LoreError::EmptyName);
                    }
                    if clean.iter().any(|seen| Self::same_name(seen, trimmed))
                        || Self::same_name(&next_name, trimmed)
                        || self
                            .claimed(Some(id))
                            .iter()
                            .any(|taken| taken == &trimmed.to_lowercase())
                    {
                        return Err(LoreError::DuplicateName(trimmed.to_string()));
                    }
                    clean.push(trimmed.to_string());
                }
                clean
            }
            None => current.aliases.clone(),
        };
        // All checks passed: apply.
        if let Some(entity) = self.entities.get_mut(id) {
            entity.name = next_name;
            entity.aliases = next_aliases;
            if let Some(text) = sheet {
                entity.sheet = text.to_string();
            }
        }
        Ok(())
    }

    /// Replace an entity's lore sheet.
    ///
    /// # Errors
    /// `UnknownEntity`/`InactiveEntity`.
    pub fn set_sheet(&mut self, id: EntityId, sheet: &str) -> Result<(), LoreError> {
        match self.entities.get_mut(id) {
            Some(entity) if entity.alive => {
                entity.sheet = sheet.to_string();
                Ok(())
            }
            Some(_) => Err(LoreError::InactiveEntity(id)),
            None => Err(LoreError::UnknownEntity(id)),
        }
    }

    /// Remove an entity (tombstone; mentions already written keep rendering
    /// as unresolved rather than dangling).
    ///
    /// # Errors
    /// `UnknownEntity`/`InactiveEntity`.
    pub fn remove(&mut self, id: EntityId) -> Result<(), LoreError> {
        match self.entities.get_mut(id) {
            Some(entity) if entity.alive => {
                entity.alive = false;
                Ok(())
            }
            Some(_) => Err(LoreError::InactiveEntity(id)),
            None => Err(LoreError::UnknownEntity(id)),
        }
    }

    /// Exact resolution by name or alias (case-insensitive), live only.
    #[must_use]
    pub fn resolve(&self, name: &str) -> Option<&Entity> {
        self.entities.iter().find(|entity| {
            entity.alive
                && (Self::same_name(&entity.name, name)
                    || entity
                        .aliases
                        .iter()
                        .any(|alias| Self::same_name(alias, name)))
        })
    }

    /// Live entities whose name *or alias* starts with `prefix`
    /// (case-insensitive), name matches before alias matches.
    #[must_use]
    pub fn find_by_prefix(&self, prefix: &str) -> Vec<&Entity> {
        let needle = prefix.to_lowercase();
        let mut names = Vec::new();
        let mut aliases = Vec::new();
        for entity in self.entities.iter().filter(|entity| entity.alive) {
            if entity.name.to_lowercase().starts_with(&needle) {
                names.push(entity);
            } else if entity
                .aliases
                .iter()
                .any(|alias| alias.to_lowercase().starts_with(&needle))
            {
                aliases.push(entity);
            }
        }
        names.extend(aliases);
        names
    }

    /// Import scene POVs as characters and settings as places, skipping
    /// blanks and anything already registered. Returns new entity count.
    /// Called by the frontend whenever the manuscript gains structure.
    pub fn seed_from_manuscript(&mut self, manuscript: &Manuscript) -> usize {
        let mut added: usize = 0;
        let mut seen: Vec<String> = Vec::new();
        let mut consider =
            |book: &mut Self, name: &str, kind: EntityKind, seen: &mut Vec<String>| {
                let name = name.trim();
                if name.is_empty() || seen.iter().any(|s| Self::same_name(s, name)) {
                    return;
                }
                seen.push(name.to_string());
                if book.add(kind, name).is_ok() {
                    added = added.saturating_add(1);
                }
            };
        // Walk the whole tree (acts → chapters → scenes) via children().
        let root = manuscript.root();
        let acts: Vec<usize> = manuscript.children(root).iter().map(|n| n.id).collect();
        for act in acts {
            let chapters: Vec<usize> = manuscript.children(act).iter().map(|n| n.id).collect();
            for chapter in chapters {
                let scenes: Vec<usize> =
                    manuscript.children(chapter).iter().map(|n| n.id).collect();
                for scene in scenes {
                    if let Some(node) = manuscript.get(scene) {
                        if let Some(meta) = node.meta.as_ref() {
                            consider(self, &meta.pov.clone(), EntityKind::Character, &mut seen);
                            consider(self, &meta.setting.clone(), EntityKind::Place, &mut seen);
                        }
                    }
                }
            }
        }
        added
    }

    /// Scan `text` for `@mentions`. At each `@`, the longest registered
    /// name/alias matching the following text (case-insensitive, ending on a
    /// word boundary) wins; otherwise a single raw token is taken so the UI
    /// can offer creation. A trailing `@` with no token is ignored.
    #[must_use]
    pub fn parse_mentions(&self, text: &str) -> Vec<Mention> {
        let mut mentions = Vec::new();
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'@' {
                i = i.saturating_add(1);
                continue;
            }
            let token_start = i.saturating_add(1);
            let starts_token = text
                .get(token_start..)
                .and_then(|rest| rest.chars().next())
                .is_some_and(is_mention_char);
            if !starts_token {
                i = token_start;
                continue;
            }
            let rest = &text[token_start..];
            // Longest registered name first (byte slices guarded: registry
            // names may not align with multibyte boundaries in `rest`).
            let mut best: Option<(usize, EntityId, String)> = None;
            for entity in self.entities.iter().filter(|e| e.alive) {
                for candidate in std::iter::once(&entity.name).chain(entity.aliases.iter()) {
                    let prefix = rest.get(..candidate.len());
                    let tail = rest.get(candidate.len()..);
                    if let (Some(head), Some(tail)) = (prefix, tail) {
                        if head.eq_ignore_ascii_case(candidate)
                            && is_word_end(tail)
                            && best
                                .as_ref()
                                .is_none_or(|(len, _, _)| candidate.len() > *len)
                        {
                            best = Some((candidate.len(), entity.id, candidate.clone()));
                        }
                    }
                }
            }
            if let Some((len, id, name)) = best {
                mentions.push(Mention {
                    byte_range: i..token_start.saturating_add(len),
                    name,
                    entity: Some(id),
                });
                i = token_start.saturating_add(len);
            } else {
                // Raw single token, walked by `char` so multibyte text
                // can never split a boundary: letters/digits plus _ ' - .
                let mut end = token_start;
                for (rel, ch) in text[token_start..].char_indices() {
                    if !is_mention_char(ch) {
                        break;
                    }
                    end = token_start
                        .saturating_add(rel)
                        .saturating_add(ch.len_utf8());
                }
                mentions.push(Mention {
                    byte_range: i..end,
                    name: text[token_start..end].to_string(),
                    entity: None,
                });
                i = end.max(token_start.saturating_add(1));
            }
        }
        mentions
    }

    /// Number of live entities.
    #[must_use]
    pub fn live_count(&self) -> usize {
        self.entities.iter().filter(|entity| entity.alive).count()
    }
}

/// Mention-token characters for *unresolved* mentions (single token only).
/// Public so frontends apply the exact same rule when extracting the live
/// `@query` behind the cursor.
#[must_use]
pub fn is_mention_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '\'' | '-' | '.')
}

/// A registered name match must end on a word boundary (end of text,
/// whitespace, or punctuation that cannot extend the name). A trailing `.`
/// is sentence punctuation, not part of the name (`@Joren.` mentions Joren);
/// `-`/`'`/`_` stay boundary-sensitive (`Anne-Marie`, `O'Brien`).
fn is_word_end(rest: &str) -> bool {
    rest.chars()
        .next()
        .is_none_or(|ch| !(ch.is_alphanumeric() || matches!(ch, '_' | '\'' | '-')))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stocked() -> LoreBook {
        let mut book = LoreBook::new();
        let mara = book.add(EntityKind::Character, "Mara Stone").unwrap();
        book.add_alias(mara, "Mara").unwrap();
        book.add(EntityKind::Place, "Mill farm").unwrap();
        book
    }

    #[test]
    fn names_must_be_unique_case_insensitively() {
        let mut book = LoreBook::new();
        book.add(EntityKind::Character, "Mara").unwrap();
        assert_eq!(
            book.add(EntityKind::Place, "mara"),
            Err(LoreError::DuplicateName("mara".to_string()))
        );
        assert_eq!(
            book.add(EntityKind::Character, "  "),
            Err(LoreError::EmptyName)
        );
    }

    #[test]
    fn resolve_hits_names_and_aliases() {
        let book = stocked();
        assert_eq!(
            book.resolve("mara stone").unwrap().kind,
            EntityKind::Character
        );
        assert_eq!(book.resolve("MARA").unwrap().name, "Mara Stone");
        assert_eq!(book.resolve("mill FARM").unwrap().kind, EntityKind::Place);
        assert!(book.resolve("Gandalf").is_none());
    }

    #[test]
    fn prefix_search_ranks_names_before_aliases() {
        let book = stocked();
        let hits: Vec<&str> = book
            .find_by_prefix("mar")
            .into_iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(hits, vec!["Mara Stone"]);
        let hits: Vec<&str> = book
            .find_by_prefix("mill")
            .into_iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(hits, vec!["Mill farm"]);
        assert!(book.find_by_prefix("zzz").is_empty());
    }

    #[test]
    fn longest_registered_name_wins() {
        let book = stocked();
        let mentions = book.parse_mentions("met @Mara Stone at dawn");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].name, "Mara Stone");
        assert!(mentions[0].entity.is_some());
        // Alias also resolves to the same entity.
        let mentions = book.parse_mentions("@Mara!");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].name, "Mara");
    }

    #[test]
    fn sentence_final_period_does_not_extend_names() {
        let mut book = stocked();
        book.add(EntityKind::Character, "Joren").unwrap();
        // "@Joren." mentions Joren — the dot is punctuation, not the name.
        let mentions = book.parse_mentions(" waved at @Joren.");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].name, "Joren");
        assert!(mentions[0].entity.is_some());
    }

    #[test]
    fn unknown_mentions_parse_as_unresolved_tokens() {
        let book = stocked();
        let mentions = book.parse_mentions("ask @Gandalf the Grey, ok?");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].name, "Gandalf");
        assert_eq!(mentions[0].entity, None);
        // Lone @ and email-like text are ignored.
        assert!(book.parse_mentions("email me @ home").is_empty());
        assert!(book.parse_mentions("trailing @").is_empty());
    }

    #[test]
    fn removal_tombstones_without_dangling() {
        let mut book = stocked();
        let id = book.resolve("Mara Stone").unwrap().id;
        book.remove(id).unwrap();
        assert!(book.resolve("mara").is_none());
        assert!(book.find_by_prefix("mar").is_empty());
        // Written mentions still parse (now unresolved).
        let mentions = book.parse_mentions("@Mara Stone");
        assert_eq!(mentions[0].entity, None);
    }

    #[test]
    fn seed_imports_povs_and_settings_once() {
        use super::super::manuscript::{Manuscript, SceneMeta};
        let mut ms = Manuscript::new("T");
        let act = ms.add_act("A").unwrap();
        let ch = ms.add_chapter(act, "C").unwrap();
        for title in ["S1", "S2"] {
            let sc = ms.add_scene(ch, title).unwrap();
            ms.set_meta(
                sc,
                SceneMeta {
                    pov: "Mara".to_string(),
                    setting: "Mill farm".to_string(),
                    ..SceneMeta::default()
                },
            )
            .unwrap();
        }
        let mut book = LoreBook::new();
        assert_eq!(book.seed_from_manuscript(&ms), 2);
        // Second seed imports nothing new.
        assert_eq!(book.seed_from_manuscript(&ms), 0);
        assert_eq!(book.live_count(), 2);
    }

    #[test]
    fn update_validates_and_applies() {
        let mut book = stocked();
        let id = book.resolve("Mara Stone").unwrap().id;
        book.update(
            id,
            Some("Mara the Bold"),
            Some(&["Bold Mara".to_string()]),
            Some("A legendary explorer."),
        )
        .unwrap();
        let entity = book.get(id).unwrap();
        assert_eq!(entity.name, "Mara the Bold");
        assert_eq!(entity.aliases, vec!["Bold Mara"]);
        assert_eq!(entity.sheet, "A legendary explorer.");

        // Duplicate name rejection against other entity
        assert_eq!(
            book.update(id, Some("Mill farm"), None, None),
            Err(LoreError::DuplicateName("Mill farm".to_string()))
        );
        // Duplicate alias rejection against own name
        assert_eq!(
            book.update(id, None, Some(&["Mara the Bold".to_string()]), None),
            Err(LoreError::DuplicateName("Mara the Bold".to_string()))
        );
        // Empty name rejection
        assert_eq!(
            book.update(id, Some("   "), None, None),
            Err(LoreError::EmptyName)
        );
    }

    #[test]
    fn lore_error_formatting_and_kind_parse() {
        assert_eq!(
            lore_error_message(&LoreError::DuplicateName("Mara".to_string())),
            "name already in use: Mara"
        );
        assert_eq!(
            lore_error_message(&LoreError::EmptyName),
            "name cannot be empty"
        );
        assert_eq!(EntityKind::parse("Character"), Some(EntityKind::Character));
        assert_eq!(EntityKind::parse(" place "), Some(EntityKind::Place));
        assert_eq!(EntityKind::parse("faction"), Some(EntityKind::Faction));
        assert_eq!(EntityKind::parse("item"), Some(EntityKind::Item));
        assert_eq!(EntityKind::parse("lore"), Some(EntityKind::Lore));
        assert_eq!(EntityKind::parse("invalid"), None);
    }
}
