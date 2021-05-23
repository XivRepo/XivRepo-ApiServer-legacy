use crate::database::models;
use crate::models::users::{Role, User, UserId};
use actix_web::http::HeaderMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthenticationError {
    #[error("An unknown database error occurred")]
    SqlxDatabaseError(#[from] sqlx::Error),
    #[error("Database Error: {0}")]
    DatabaseError(#[from] crate::database::models::DatabaseError),
    #[error("Error while parsing JSON: {0}")]
    SerDeError(#[from] serde_json::Error),
    #[error("Error while communicating to Discord OAuth2: {0}")]
    GithubError(#[from] reqwest::Error),
    #[error("Invalid Authentication Credentials")]
    InvalidCredentialsError,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
    pub avatar: String,
    pub discriminator: String,
    pub locale: String,
    pub email: Option<String>,
    pub verified: bool,
}

pub async fn get_discord_user_from_token(
    access_token: &str,
) -> Result<DiscordUser, AuthenticationError> {
    Ok(reqwest::Client::new()
        .get("https://discord.com/api/users/@me")
        .header(reqwest::header::USER_AGENT, "Modrinth")
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", access_token),
        )
        .send()
        .await?
        .json()
        .await?)
}

pub async fn get_user_from_token<'a, 'b, E>(
    access_token: &str,
    executor: E,
) -> Result<User, AuthenticationError>
where
    E: sqlx::Executor<'a, Database = sqlx::Postgres>,
{
    let discord_user = get_discord_user_from_token(access_token).await?;

    let res = models::User::get_from_discord_id(discord_user.id, executor).await?;

    match res {
        Some(result) => Ok(User {
            id: UserId::from(result.id),
            discord_id: result.discord_id,
            username: result.username,
            name: result.name,
            email: result.email,
            avatar_url: result.avatar_url,
            bio: result.bio,
            created: result.created,
            role: Role::from_string(&*result.role),
        }),
        None => Err(AuthenticationError::InvalidCredentialsError),
    }
}
pub async fn get_user_from_headers<'a, 'b, E>(
    headers: &HeaderMap,
    executor: E,
) -> Result<User, AuthenticationError>
where
    E: sqlx::Executor<'a, Database = sqlx::Postgres>,
{
    let token = headers
        .get("Authorization")
        .ok_or(AuthenticationError::InvalidCredentialsError)?
        .to_str()
        .map_err(|_| AuthenticationError::InvalidCredentialsError)?;

    Ok(get_user_from_token(token, executor).await?)
}

pub async fn check_is_moderator_from_headers<'a, 'b, E>(
    headers: &HeaderMap,
    executor: E,
) -> Result<User, AuthenticationError>
where
    E: sqlx::Executor<'a, Database = sqlx::Postgres>,
{
    let user = get_user_from_headers(headers, executor).await?;

    if user.role.is_mod() {
        Ok(user)
    } else {
        Err(AuthenticationError::InvalidCredentialsError)
    }
}

pub async fn check_is_admin_from_headers<'a, 'b, E>(
    headers: &HeaderMap,
    executor: E,
) -> Result<User, AuthenticationError>
where
    E: sqlx::Executor<'a, Database = sqlx::Postgres>,
{
    let user = get_user_from_headers(headers, executor).await?;

    match user.role {
        Role::Admin => Ok(user),
        _ => Err(AuthenticationError::InvalidCredentialsError),
    }
}
