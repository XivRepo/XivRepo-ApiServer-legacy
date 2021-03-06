use super::ids::Base62Id;
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Base62Id")]
#[serde(into = "Base62Id")]
pub struct UserId(pub u64);

pub const DELETED_USER: UserId = UserId(127155982985829);

#[derive(Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub discord_id: Option<String>,
    pub username: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub created: chrono::DateTime<chrono::Utc>,
    pub role: Role,
    pub show_nsfw: bool
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Developer,
    Moderator,
    Admin,
}

impl std::fmt::Display for Role {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Role::Developer => write!(fmt, "developer"),
            Role::Moderator => write!(fmt, "moderator"),
            Role::Admin => write!(fmt, "admin"),
        }
    }
}

impl Role {
    pub fn from_string(string: &str) -> Role {
        match string {
            "admin" => Role::Admin,
            "moderator" => Role::Moderator,
            _ => Role::Developer,
        }
    }

    pub fn is_mod(&self) -> bool {
        match self {
            Role::Developer => false,
            Role::Moderator | Role::Admin => true,
        }
    }
}
