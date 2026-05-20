use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KanbanCard {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// The scanned project path this card belongs to (may become stale if the
    /// project is removed from scan roots).
    #[serde(default)]
    pub project_path: Option<String>,
    /// Label of the assigned agent preset (or "Claude" / "Codex" for built-in).
    #[serde(default)]
    pub assigned_agent: Option<String>,
    /// Path to a git worktree, if one was created for this card.
    #[serde(default)]
    pub worktree_path: Option<String>,
    /// Which column this card lives in (column id).
    pub column_id: String,
    /// Ordinal for sorting within a column (lower = first).
    pub position: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KanbanColumn {
    pub id: String,
    pub name: String,
    pub position: u32,
    #[serde(default)]
    pub wip_limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentBlock {
    pub agent_label: String,
    /// Unix millisecond timestamp when the block ends, or None if not blocked.
    #[serde(default)]
    pub blocked_until: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KanbanBoard {
    #[serde(default)]
    pub columns: Vec<KanbanColumn>,
    #[serde(default)]
    pub cards: Vec<KanbanCard>,
    #[serde(default)]
    pub agent_blocks: Vec<AgentBlock>,
}

impl KanbanBoard {
    /// Return the default board with the standard columns.
    pub fn default_board() -> Self {
        Self {
            columns: vec![
                KanbanColumn { id: "col-design".into(), name: "Feature design".into(), position: 0, wip_limit: None },
                KanbanColumn { id: "col-coding".into(), name: "LLM Coding".into(), position: 1, wip_limit: None },
                KanbanColumn { id: "col-review".into(), name: "Human verification".into(), position: 2, wip_limit: None },
                KanbanColumn { id: "col-commit".into(), name: "Commit building and rebasing".into(), position: 3, wip_limit: None },
            ],
            cards: vec![],
            agent_blocks: vec![],
        }
    }

    pub fn get_column(&self, id: &str) -> Option<&KanbanColumn> {
        self.columns.iter().find(|c| c.id == id)
    }

    pub fn get_column_mut(&mut self, id: &str) -> Option<&mut KanbanColumn> {
        self.columns.iter_mut().find(|c| c.id == id)
    }

    pub fn get_card(&self, id: &str) -> Option<&KanbanCard> {
        self.cards.iter().find(|c| c.id == id)
    }

    pub fn get_card_mut(&mut self, id: &str) -> Option<&mut KanbanCard> {
        self.cards.iter_mut().find(|c| c.id == id)
    }

    pub fn cards_in_column(&self, column_id: &str) -> Vec<&KanbanCard> {
        let mut cards: Vec<&KanbanCard> = self.cards.iter().filter(|c| c.column_id == column_id).collect();
        cards.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap_or(std::cmp::Ordering::Equal));
        cards
    }

    pub fn columns_sorted(&self) -> Vec<&KanbanColumn> {
        let mut cols: Vec<&KanbanColumn> = self.columns.iter().collect();
        cols.sort_by_key(|c| c.position);
        cols
    }

    /// Group cards in a column by project path, preserving card sort order.
    pub fn cards_in_column_grouped(&self, column_id: &str) -> Vec<(&str, Vec<&KanbanCard>)> {
        let cards = self.cards_in_column(column_id);
        let mut groups: Vec<(&str, Vec<&KanbanCard>)> = Vec::new();
        for card in &cards {
            let proj = card.project_path.as_deref().unwrap_or("");
            if let Some(last) = groups.last_mut() {
                if last.0 == proj {
                    last.1.push(card);
                    continue;
                }
            }
            groups.push((proj, vec![card]));
        }
        groups
    }

    pub fn is_agent_blocked(&self, agent_label: &str) -> Option<i64> {
        let now = chrono::Utc::now().timestamp_millis();
        self.agent_blocks
            .iter()
            .find(|ab| ab.agent_label == agent_label)
            .and_then(|ab| ab.blocked_until.filter(|&ts| ts > now))
    }

    /// Set or clear a block for an agent.
    /// Pass `None` to clear the block, or `Some(millis)` to set/update it.
    pub fn set_agent_block(&mut self, agent_label: &str, blocked_until: Option<i64>) {
        if let Some(block) = self
            .agent_blocks
            .iter_mut()
            .find(|ab| ab.agent_label == agent_label)
        {
            match blocked_until {
                Some(ts) => block.blocked_until = Some(ts),
                None => {
                    self.agent_blocks.retain(|ab| ab.agent_label != agent_label);
                }
            }
        } else if let Some(ts) = blocked_until {
            self.agent_blocks.push(AgentBlock {
                agent_label: agent_label.to_string(),
                blocked_until: Some(ts),
            });
        }
    }

    /// Get the block for an agent, if any.
    pub fn get_agent_block(&self, agent_label: &str) -> Option<&AgentBlock> {
        self.agent_blocks
            .iter()
            .find(|ab| ab.agent_label == agent_label)
    }
}
