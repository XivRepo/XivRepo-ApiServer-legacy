use std::collections::HashMap;

use crate::auth::get_discord_user_from_token;
use crate::database::models::{generate_state_id, User};
use crate::models::error::ApiError;
use crate::models::ids::base62_impl::{parse_base62, to_base62};
use crate::models::ids::DecodingError;
use crate::models::users::Role;
use actix_web::http::StatusCode;
use actix_web::web::{scope, Data, Query, ServiceConfig};
use actix_web::{get, HttpResponse};
use chrono::Utc;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;
use thiserror::Error;

pub fn config(cfg: &mut ServiceConfig) {
    cfg.service(scope("/auth/").service(auth_callback).service(init));
}

#[derive(Error, Debug)]
pub enum AuthorizationError {
    #[error("Environment Error")]
    EnvError(#[from] dotenv::Error),
    #[error("An unknown database error occured: {0}")]
    SqlxDatabaseError(#[from] sqlx::Error),
    #[error("Database Error: {0}")]
    DatabaseError(#[from] crate::database::models::DatabaseError),
    #[error("Error while parsing JSON: {0}")]
    SerDeError(#[from] serde_json::Error),
    #[error("Error while communicating to Discord OAuth2: {0}")]
    GithubError(#[from] reqwest::Error),
    #[error("Invalid Authentication credentials")]
    InvalidCredentialsError,
    #[error("Authentication Error: {0}")]
    AuthenticationError(#[from] crate::auth::AuthenticationError),
    #[error("Error while decoding Base62")]
    DecodingError(#[from] DecodingError),
}
impl actix_web::ResponseError for AuthorizationError {
    fn status_code(&self) -> StatusCode {
        match self {
            AuthorizationError::EnvError(..) => StatusCode::INTERNAL_SERVER_ERROR,
            AuthorizationError::SqlxDatabaseError(..) => StatusCode::INTERNAL_SERVER_ERROR,
            AuthorizationError::DatabaseError(..) => StatusCode::INTERNAL_SERVER_ERROR,
            AuthorizationError::SerDeError(..) => StatusCode::BAD_REQUEST,
            AuthorizationError::GithubError(..) => StatusCode::FAILED_DEPENDENCY,
            AuthorizationError::InvalidCredentialsError => StatusCode::UNAUTHORIZED,
            AuthorizationError::DecodingError(..) => StatusCode::BAD_REQUEST,
            AuthorizationError::AuthenticationError(..) => StatusCode::UNAUTHORIZED,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(ApiError {
            error: match self {
                AuthorizationError::EnvError(..) => "environment_error",
                AuthorizationError::SqlxDatabaseError(..) => "database_error",
                AuthorizationError::DatabaseError(..) => "database_error",
                AuthorizationError::SerDeError(..) => "invalid_input",
                AuthorizationError::GithubError(..) => "discord_error",
                AuthorizationError::InvalidCredentialsError => "invalid_credentials",
                AuthorizationError::DecodingError(..) => "decoding_error",
                AuthorizationError::AuthenticationError(..) => "authentication_error",
            },
            description: &self.to_string(),
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct AuthorizationInit {
    pub url: String,
}

#[derive(Serialize, Deserialize)]
pub struct Authorization {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>
}

#[derive(Serialize, Deserialize)]
pub struct AccessToken {
    pub access_token: String,
    pub scope: String,
    pub token_type: String,
}

//http://localhost:8000/api/v1/auth/init?url=siteurl
#[get("init")]
pub async fn init(
    Query(info): Query<AuthorizationInit>,
    client: Data<PgPool>,
) -> Result<HttpResponse, AuthorizationError> {
    let mut transaction = client.begin().await?;

    let state = generate_state_id(&mut transaction).await?;

    sqlx::query!(
        "
            INSERT INTO states (id, url)
            VALUES ($1, $2)
            ",
        state.0,
        info.url
    )
    .execute(&mut *transaction)
    .await?;

    transaction.commit().await?;

    let client_id = dotenv::var("DISCORD_CLIENT_ID")?;
    let redirect_uri = dotenv::var("DISCORD_REDIRECT_URI")?;
    let url = format!(
        "https://discord.com/oauth2/authorize?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
        client_id,
        redirect_uri,
        "identify%20email",
        to_base62(state.0 as u64)
    );

    Ok(HttpResponse::TemporaryRedirect()
        .header("Location", &*url)
        .json(AuthorizationInit { url }))
}

#[get("callback")]
pub async fn auth_callback(
    Query(info): Query<Authorization>,
    client: Data<PgPool>,
) -> Result<HttpResponse, AuthorizationError> {
    let mut transaction = client.begin().await?;

    if info.error.is_some() {
        let error = info.error.unwrap();
        warn!("Error authorizing Discord login : {}", error);

        let home = dotenv::var("SITE_URL")?;

        Ok(HttpResponse::TemporaryRedirect()
            .header("Location", home)
            .finish())
    }
    else {
        let state_id = parse_base62(&*info.state.unwrap())?;

        let result = sqlx::query!(
            "
                SELECT url,expires FROM states
                WHERE id = $1
                ",
            state_id as i64
        )
        .fetch_one(&mut *transaction)
        .await?;

        let now = Utc::now();
        let duration = result.expires.signed_duration_since(now);

        if duration.num_seconds() < 0 {
            return Err(AuthorizationError::InvalidCredentialsError);
        }

        sqlx::query!(
            "
                DELETE FROM states
                WHERE id = $1
                ",
            state_id as i64
        )
        .execute(&mut *transaction)
        .await?;

        let client_id = dotenv::var("DISCORD_CLIENT_ID")?;
        let client_secret = dotenv::var("DISCORD_CLIENT_SECRET")?;
        let redirect_uri = dotenv::var("DISCORD_REDIRECT_URI")?;

        let url = format!("https://discord.com/api/v8/oauth2/token");
        let code = info.code.unwrap();

        let mut params = HashMap::new();
        params.insert("client_id", client_id);
        params.insert("client_secret", client_secret);
        params.insert("grant_type", "authorization_code".into());
        params.insert("code", code);
        params.insert("redirect_uri", redirect_uri);

        let token: AccessToken = reqwest::Client::new()
            .post(&url)
            .header(reqwest::header::ACCEPT, "application/json")
            .form(&params)
            .send()
            .await?
            .json()
            .await?;

        let user = get_discord_user_from_token(&*token.access_token).await?;

        let user_result = User::get_from_discord_id(user.id.clone(), &mut *transaction).await?;
        match user_result {
            Some(x) => info!("{:?}", x.id),
            None => {
                let user_id = crate::database::models::generate_user_id(&mut transaction).await?;

                User {
                    id: user_id,
                    discord_id: Some(user.id.clone()),
                    username: user.username.clone(),
                    name: Some(user.username),
                    email: user.email,
                    avatar_url: Some(format!("https://cdn.discordapp.com/avatars/{}/{}",user.id, user.avatar)),
                    bio: None,
                    created: Utc::now(),
                    role: Role::Developer.to_string(),
                }
                .insert(&mut transaction)
                .await?;
            }
        }

        transaction.commit().await?;

        let redirect_url = format!("{}?code={}", result.url, token.access_token);

        Ok(HttpResponse::TemporaryRedirect()
            .header("Location", &*redirect_url)
            .json(AuthorizationInit { url: redirect_url }))
    }
}
