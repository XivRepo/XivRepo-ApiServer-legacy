use crate::auth::get_user_from_headers;
use crate::database;
use crate::file_hosting::FileHost;
use crate::models;
use crate::models::mods::{DonationLink, ModId, ModStatus, SearchRequest, Dependency, DependencyType};
use crate::models::teams::Permissions;
use crate::routes::ApiError;
use crate::search::indexing::queue::CreationQueue;
use crate::search::{search_for_mod, SearchConfig, SearchError};
use actix_web::web::Data;
use actix_web::{delete, get, patch, post, web, HttpRequest, HttpResponse};
use futures::StreamExt;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

#[get("mod")]
pub async fn mod_search(
    web::Query(info): web::Query<SearchRequest>,
    config: web::Data<SearchConfig>,
    req: HttpRequest,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, SearchError> {
    let user = get_user_from_headers(&req.headers(), &**pool).await.ok();
    let results = search_for_mod(&info, &**config, user).await?;
    Ok(HttpResponse::Ok().json(results))
}

#[derive(Serialize, Deserialize)]
pub struct ModIds {
    pub ids: String,
}

// TODO: Make this return the full mod struct
#[get("mods")]
pub async fn mods_get(
    req: HttpRequest,
    web::Query(ids): web::Query<ModIds>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ApiError> {
    let mod_ids = serde_json::from_str::<Vec<models::ids::ModId>>(&*ids.ids)?
        .into_iter()
        .map(|x| x.into())
        .collect();

    let mods_data = database::models::Mod::get_many_full(mod_ids, &**pool)
        .await
        .map_err(|e| ApiError::DatabaseError(e.into()))?;

    let user_option = get_user_from_headers(req.headers(), &**pool).await.ok();

    let mut mods = Vec::new();

    for mod_data in mods_data {
        let mut authorized = !mod_data.status.is_hidden();

        if let Some(user) = &user_option {
            if !authorized {
                if user.role.is_mod() {
                    authorized = true;
                } else {
                    let user_id: database::models::ids::UserId = user.id.into();

                    let mod_exists = sqlx::query!(
                            "SELECT EXISTS(SELECT 1 FROM team_members WHERE team_id = $1 AND user_id = $2)",
                            mod_data.inner.team_id as database::models::ids::TeamId,
                            user_id as database::models::ids::UserId,
                        )
                        .fetch_one(&**pool)
                        .await
                        .map_err(|e| ApiError::DatabaseError(e.into()))?
                        .exists;

                    authorized = mod_exists.unwrap_or(false);
                }
            }
        }

        if authorized {
            mods.push(convert_mod(mod_data));
        }
    }

    Ok(HttpResponse::Ok().json(mods))
}

#[get("@{id}")]
pub async fn mod_slug_get(
    req: HttpRequest,
    info: web::Path<(String,)>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ApiError> {
    let id = info.into_inner().0;
    let mod_data = database::models::Mod::get_full_from_slug(&id, &**pool)
        .await
        .map_err(|e| ApiError::DatabaseError(e.into()))?;
    let user_option = get_user_from_headers(req.headers(), &**pool).await.ok();

    if let Some(data) = mod_data {
        let mut authorized = !data.status.is_hidden();

        if let Some(user) = user_option {
            if !authorized {
                if user.role.is_mod() {
                    authorized = true;
                } else {
                    let user_id: database::models::ids::UserId = user.id.into();

                    let mod_exists = sqlx::query!(
                        "SELECT EXISTS(SELECT 1 FROM team_members WHERE team_id = $1 AND user_id = $2)",
                        data.inner.team_id as database::models::ids::TeamId,
                        user_id as database::models::ids::UserId,
                    )
                    .fetch_one(&**pool)
                    .await
                    .map_err(|e| ApiError::DatabaseError(e.into()))?
                    .exists;

                    authorized = mod_exists.unwrap_or(false);
                }
            }
        }

        if authorized {
            return Ok(HttpResponse::Ok().json(convert_mod(data)));
        }

        Ok(HttpResponse::NotFound().body(""))
    } else {
        Ok(HttpResponse::NotFound().body(""))
    }
}

#[get("{id}")]
pub async fn mod_get(
    req: HttpRequest,
    info: web::Path<(String,)>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ApiError> {
    let string = info.into_inner().0;
    let id_option: Option<ModId> = serde_json::from_str(&*format!("\"{}\"", string)).ok();

    let mut mod_data;

    if let Some(id) = id_option {
        mod_data = database::models::Mod::get_full(id.into(), &**pool)
            .await
            .map_err(|e| ApiError::DatabaseError(e.into()))?;

        if mod_data.is_none() {
            mod_data = database::models::Mod::get_full_from_slug(&string, &**pool)
                .await
                .map_err(|e| ApiError::DatabaseError(e.into()))?;
        }
    } else {
        mod_data = database::models::Mod::get_full_from_slug(&string, &**pool)
            .await
            .map_err(|e| ApiError::DatabaseError(e.into()))?;
    }

    let user_option = get_user_from_headers(req.headers(), &**pool).await.ok();

    if let Some(data) = mod_data {
        let mut authorized = !data.status.is_hidden();

        if let Some(user) = user_option {
            if !authorized {
                if user.role.is_mod() {
                    authorized = true;
                } else {
                    let user_id: database::models::ids::UserId = user.id.into();

                    let mod_exists = sqlx::query!(
                        "SELECT EXISTS(SELECT 1 FROM team_members WHERE team_id = $1 AND user_id = $2)",
                        data.inner.team_id as database::models::ids::TeamId,
                        user_id as database::models::ids::UserId,
                    )
                    .fetch_one(&**pool)
                    .await
                    .map_err(|e| ApiError::DatabaseError(e.into()))?
                    .exists;

                    authorized = mod_exists.unwrap_or(false);
                }
            }
        }

        if authorized {
            return Ok(HttpResponse::Ok().json(convert_mod(data)));
        }

        Ok(HttpResponse::NotFound().body(""))
    } else {
        Ok(HttpResponse::NotFound().body(""))
    }
}

#[get("random_mod")]
pub async fn mod_get_random(
    req: HttpRequest,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ApiError> {
    let mod_data;

    let mut rng = rand::thread_rng();

    let user_option = get_user_from_headers(req.headers(), &**pool).await.ok();
    let mut allow_nsfw = false;

    if let Some(user) = &user_option {
        if user.show_nsfw {
            allow_nsfw = true;
        }
    }

    let mod_count;

    if !allow_nsfw {
        mod_count = sqlx::query!(
            "SELECT COUNT(*) FROM mods WHERE is_nsfw = false"
        )
        .fetch_one(&**pool)
        .await
        .map_err(|e| ApiError::DatabaseError(e.into()))?
        .count;
    }
    else {
        mod_count = sqlx::query!(
            "SELECT COUNT(*) FROM mods"
        )
        .fetch_one(&**pool)
        .await
        .map_err(|e| ApiError::DatabaseError(e.into()))?
        .count;
    }

    let rand_id = rng.gen_range(1..=mod_count.unwrap());

    let result;

    if allow_nsfw {
        result = sqlx::query!(
            "SELECT x.id id FROM 
                ( 
                    SELECT id, ROW_NUMBER() OVER (ORDER BY published) 
                    FROM mods
                    WHERE status = 1
                    AND is_nsfw IS NOT NULL
                ) x 
            WHERE ROW_NUMBER = $1",
            rand_id
        )
        .fetch_one(&**pool)
        .await
        .map_err(|e| ApiError::DatabaseError(e.into()))?
        .id;
    }
    else {
        result = sqlx::query!(
            "SELECT x.id id FROM 
                ( 
                    SELECT id, ROW_NUMBER() OVER (ORDER BY published) 
                    FROM mods
                    WHERE status = 1
                    AND is_nsfw IS FALSE
                ) x 
            WHERE ROW_NUMBER = $1",
            rand_id
        )
        .fetch_one(&**pool)
        .await
        .map_err(|e| ApiError::DatabaseError(e.into()))?
        .id;
    }

    let mod_id = ModId(result as u64);

    let id_option: Option<ModId> = serde_json::from_str(&*format!("\"{}\"", mod_id)).ok();

    if let Some(id) = id_option {
        mod_data = database::models::Mod::get_full(id.into(), &**pool)
            .await
            .map_err(|e| ApiError::DatabaseError(e.into()))?
    }
    else {
        mod_data = None;
    }

    if let Some(data) = mod_data {
        let mut authorized = !data.status.is_hidden();

        if let Some(user) = user_option {
            if !authorized {
                if user.role.is_mod() {
                    authorized = true;
                } else {
                    let user_id: database::models::ids::UserId = user.id.into();

                    let mod_exists = sqlx::query!(
                        "SELECT EXISTS(SELECT 1 FROM team_members WHERE team_id = $1 AND user_id = $2)",
                        data.inner.team_id as database::models::ids::TeamId,
                        user_id as database::models::ids::UserId,
                    )
                    .fetch_one(&**pool)
                    .await
                    .map_err(|e| ApiError::DatabaseError(e.into()))?
                    .exists;

                    authorized = mod_exists.unwrap_or(false);
                }
            }
        }

        if authorized {
            return Ok(HttpResponse::Ok().json(convert_mod(data)));
        }

        Ok(HttpResponse::NotFound().body(""))
    } else {
        Ok(HttpResponse::NotFound().body(""))
    }
}

pub fn convert_mod(data: database::models::mod_item::QueryMod) -> models::mods::Mod {
    let m = data.inner;

    models::mods::Mod {
        id: m.id.into(),
        slug: m.slug.unwrap(),
        team: m.team_id.into(),
        title: m.title,
        description: m.description,
        body: m.body,
        body_url: m.body_url,
        published: m.published,
        updated: m.updated,
        status: data.status,
        is_nsfw: m.is_nsfw,
        downloads: m.downloads as u32,
        followers: m.follows as u32,
        categories: data.categories,
        versions: data.versions.into_iter().map(|v| v.into()).collect(),
        icon_url: m.icon_url,
        issues_url: m.issues_url,
        source_url: m.source_url,
        wiki_url: m.wiki_url,
        discord_url: m.discord_url,
        donation_urls: Some(
            data.donation_urls
                .into_iter()
                .map(|d| DonationLink {
                    id: d.platform_short,
                    platform: d.platform_name,
                    url: d.url,
                })
                .collect(),
        ),
        dependencies: if data.dependencies.is_empty() {
            None
        } else {
            // Probably find a more precise way to create the Dependency objects
            Some(
                data.dependencies.iter().map(
                    |dep_id| Dependency {
                        mod_id: ModId(dep_id.as_str().parse::<u64>().unwrap_or_default()), // add better error checking?
                        dependency_type: DependencyType::Required,
                        min_version_num: None
                    }).collect()
            )
        },
    }
}

/// A mod returned from the API
#[derive(Serialize, Deserialize)]
pub struct EditMod {
    pub title: Option<String>,
    pub description: Option<String>,
    pub body: Option<String>,
    pub status: Option<ModStatus>,
    pub categories: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "::serde_with::rust::double_option"
    )]
    pub issues_url: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "::serde_with::rust::double_option"
    )]
    pub source_url: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "::serde_with::rust::double_option"
    )]
    pub wiki_url: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "::serde_with::rust::double_option"
    )]
    pub discord_url: Option<Option<String>>,
    pub donation_urls: Option<Vec<DonationLink>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "::serde_with::rust::double_option"
    )]
    pub slug: Option<Option<String>>,
    pub is_nsfw: Option<bool>,
}

#[patch("{id}")]
pub async fn mod_edit(
    req: HttpRequest,
    info: web::Path<(models::ids::ModId,)>,
    pool: web::Data<PgPool>,
    config: web::Data<SearchConfig>,
    new_mod: web::Json<EditMod>,
    indexing_queue: Data<Arc<CreationQueue>>,
) -> Result<HttpResponse, ApiError> {
    let user = get_user_from_headers(req.headers(), &**pool).await?;

    let mod_id = info.into_inner().0;
    let id = mod_id.into();

    let result = database::models::Mod::get_full(id, &**pool)
        .await
        .map_err(|e| ApiError::DatabaseError(e.into()))?;

    if let Some(mod_item) = result {
        let team_member = database::models::TeamMember::get_from_user_id(
            mod_item.inner.team_id,
            user.id.into(),
            &**pool,
        )
        .await?;
        let permissions;

        if let Some(member) = team_member {
            permissions = Some(member.permissions)
        } else if user.role.is_mod() {
            permissions = Some(Permissions::ALL)
        } else {
            permissions = None
        }

        if let Some(perms) = permissions {
            let mut transaction = pool
                .begin()
                .await
                .map_err(|e| ApiError::DatabaseError(e.into()))?;

            if let Some(title) = &new_mod.title {
                if !perms.contains(Permissions::EDIT_DETAILS) {
                    return Err(ApiError::CustomAuthenticationError(
                        "You do not have the permissions to edit the title of this mod!"
                            .to_string(),
                    ));
                }

                if title.len() > 256 || title.len() < 3 {
                    return Err(ApiError::InvalidInputError(
                        "The mod's title must be within 3-256 characters!".to_string(),
                    ));
                }

                sqlx::query!(
                    "
                    UPDATE mods
                    SET title = $1
                    WHERE (id = $2)
                    ",
                    title,
                    id as database::models::ids::ModId,
                )
                .execute(&mut *transaction)
                .await
                .map_err(|e| ApiError::DatabaseError(e.into()))?;
            }

            if let Some(description) = &new_mod.description {
                if !perms.contains(Permissions::EDIT_DETAILS) {
                    return Err(ApiError::CustomAuthenticationError(
                        "You do not have the permissions to edit the description of this mod!"
                            .to_string(),
                    ));
                }

                if description.len() > 2048 || description.len() < 3 {
                    return Err(ApiError::InvalidInputError(
                        "The mod's description must be within 3-256 characters!".to_string(),
                    ));
                }

                sqlx::query!(
                    "
                    UPDATE mods
                    SET description = $1
                    WHERE (id = $2)
                    ",
                    description,
                    id as database::models::ids::ModId,
                )
                .execute(&mut *transaction)
                .await
                .map_err(|e| ApiError::DatabaseError(e.into()))?;
            }

            if let Some(status) = &new_mod.status {
                if !perms.contains(Permissions::EDIT_DETAILS) {
                    return Err(ApiError::CustomAuthenticationError(
                        "You do not have the permissions to edit the status of this mod!"
                            .to_string(),
                    ));
                }

                if (status == &ModStatus::Rejected || status == &ModStatus::Approved)
                    && !user.role.is_mod()
                {
                    return Err(ApiError::CustomAuthenticationError(
                        "You don't have permission to set this status".to_string(),
                    ));
                }

                let status_id = database::models::StatusId::get_id(&status, &mut *transaction)
                    .await?
                    .ok_or_else(|| {
                        ApiError::InvalidInputError(
                            "No database entry for status provided.".to_string(),
                        )
                    })?;

                sqlx::query!(
                    "
                    UPDATE mods
                    SET status = $1
                    WHERE (id = $2)
                    ",
                    status_id as database::models::ids::StatusId,
                    id as database::models::ids::ModId,
                )
                .execute(&mut *transaction)
                .await
                .map_err(|e| ApiError::DatabaseError(e.into()))?;

                if mod_item.status.is_searchable() && !status.is_searchable() {
                    delete_from_index(mod_id, config).await?;
                } else if !mod_item.status.is_searchable() && status.is_searchable() {
                    let index_mod = crate::search::indexing::local_import::query_one(
                        mod_id.into(),
                        &mut *transaction,
                    )
                    .await?;

                    indexing_queue.add(index_mod);
                }
            }

            if let Some(categories) = &new_mod.categories {
                if !perms.contains(Permissions::EDIT_DETAILS) {
                    return Err(ApiError::CustomAuthenticationError(
                        "You do not have the permissions to edit the categories of this mod!"
                            .to_string(),
                    ));
                }

                if categories.len() > 3 {
                    return Err(ApiError::InvalidInputError(
                        "The maximum number of categories for a mod is four.".to_string(),
                    ));
                }

                sqlx::query!(
                    "
                    DELETE FROM mods_categories
                    WHERE joining_mod_id = $1
                    ",
                    id as database::models::ids::ModId,
                )
                .execute(&mut *transaction)
                .await
                .map_err(|e| ApiError::DatabaseError(e.into()))?;

                for category in categories {
                    let category_id = database::models::categories::Category::get_id(
                        &category,
                        &mut *transaction,
                    )
                    .await?
                    .ok_or_else(|| {
                        ApiError::InvalidInputError(format!(
                            "Category {} does not exist.",
                            category.clone()
                        ))
                    })?;

                    sqlx::query!(
                        "
                        INSERT INTO mods_categories (joining_mod_id, joining_category_id)
                        VALUES ($1, $2)
                        ",
                        id as database::models::ids::ModId,
                        category_id as database::models::ids::CategoryId,
                    )
                    .execute(&mut *transaction)
                    .await
                    .map_err(|e| ApiError::DatabaseError(e.into()))?;
                }
            }

            if let Some(issues_url) = &new_mod.issues_url {
                if !perms.contains(Permissions::EDIT_DETAILS) {
                    return Err(ApiError::CustomAuthenticationError(
                        "You do not have the permissions to edit the issues URL of this mod!"
                            .to_string(),
                    ));
                }

                if let Some(issues) = issues_url {
                    if issues.len() > 2048 {
                        return Err(ApiError::InvalidInputError(
                            "The mod's issues url must be less than 2048 characters!".to_string(),
                        ));
                    }
                }

                sqlx::query!(
                    "
                    UPDATE mods
                    SET issues_url = $1
                    WHERE (id = $2)
                    ",
                    issues_url.as_deref(),
                    id as database::models::ids::ModId,
                )
                .execute(&mut *transaction)
                .await
                .map_err(|e| ApiError::DatabaseError(e.into()))?;
            }

            if let Some(source_url) = &new_mod.source_url {
                if !perms.contains(Permissions::EDIT_DETAILS) {
                    return Err(ApiError::CustomAuthenticationError(
                        "You do not have the permissions to edit the source URL of this mod!"
                            .to_string(),
                    ));
                }

                if let Some(source) = source_url {
                    if source.len() > 2048 {
                        return Err(ApiError::InvalidInputError(
                            "The mod's source url must be less than 2048 characters!".to_string(),
                        ));
                    }
                }

                sqlx::query!(
                    "
                    UPDATE mods
                    SET source_url = $1
                    WHERE (id = $2)
                    ",
                    source_url.as_deref(),
                    id as database::models::ids::ModId,
                )
                .execute(&mut *transaction)
                .await
                .map_err(|e| ApiError::DatabaseError(e.into()))?;
            }

            if let Some(wiki_url) = &new_mod.wiki_url {
                if !perms.contains(Permissions::EDIT_DETAILS) {
                    return Err(ApiError::CustomAuthenticationError(
                        "You do not have the permissions to edit the wiki URL of this mod!"
                            .to_string(),
                    ));
                }

                if let Some(wiki) = wiki_url {
                    if wiki.len() > 2048 {
                        return Err(ApiError::InvalidInputError(
                            "The mod's wiki url must be less than 2048 characters!".to_string(),
                        ));
                    }
                }

                sqlx::query!(
                    "
                    UPDATE mods
                    SET wiki_url = $1
                    WHERE (id = $2)
                    ",
                    wiki_url.as_deref(),
                    id as database::models::ids::ModId,
                )
                .execute(&mut *transaction)
                .await
                .map_err(|e| ApiError::DatabaseError(e.into()))?;
            }

            if let Some(discord_url) = &new_mod.discord_url {
                if !perms.contains(Permissions::EDIT_DETAILS) {
                    return Err(ApiError::CustomAuthenticationError(
                        "You do not have the permissions to edit the discord URL of this mod!"
                            .to_string(),
                    ));
                }

                if let Some(discord) = discord_url {
                    if discord.len() > 2048 {
                        return Err(ApiError::InvalidInputError(
                            "The mod's discord url must be less than 2048 characters!".to_string(),
                        ));
                    }
                }

                sqlx::query!(
                    "
                    UPDATE mods
                    SET discord_url = $1
                    WHERE (id = $2)
                    ",
                    discord_url.as_deref(),
                    id as database::models::ids::ModId,
                )
                .execute(&mut *transaction)
                .await
                .map_err(|e| ApiError::DatabaseError(e.into()))?;
            }

            if let Some(slug) = &new_mod.slug {
                if !perms.contains(Permissions::EDIT_DETAILS) {
                    return Err(ApiError::CustomAuthenticationError(
                        "You do not have the permissions to edit the slug of this mod!".to_string(),
                    ));
                }

                if let Some(slug) = slug {
                    if slug.len() > 64 || slug.len() < 3 {
                        return Err(ApiError::InvalidInputError(
                            "The mod's slug must be within 3-64 characters!".to_string(),
                        ));
                    }

                    let slug_modid_option: Option<ModId> =
                        serde_json::from_str(&*format!("\"{}\"", slug)).ok();
                    if let Some(slug_modid) = slug_modid_option {
                        let slug_modid: database::models::ids::ModId = slug_modid.into();
                        let results = sqlx::query!(
                            "
                            SELECT EXISTS(SELECT 1 FROM mods WHERE id=$1)
                            ",
                            slug_modid as database::models::ids::ModId
                        )
                        .fetch_one(&mut *transaction)
                        .await
                        .map_err(|e| ApiError::DatabaseError(e.into()))?;

                        if results.exists.unwrap_or(true) {
                            return Err(ApiError::InvalidInputError(
                                "Slug collides with other mod's id!".to_string(),
                            ));
                        }
                    }
                }

                sqlx::query!(
                    "
                    UPDATE mods
                    SET slug = LOWER($1)
                    WHERE (id = $2)
                    ",
                    slug.as_deref(),
                    id as database::models::ids::ModId,
                )
                .execute(&mut *transaction)
                .await
                .map_err(|e| ApiError::DatabaseError(e.into()))?;
            }

            if let Some(donations) = &new_mod.donation_urls {
                if !perms.contains(Permissions::EDIT_DETAILS) {
                    return Err(ApiError::CustomAuthenticationError(
                        "You do not have the permissions to edit the donation links of this mod!"
                            .to_string(),
                    ));
                }

                sqlx::query!(
                    "
                    DELETE FROM mods_donations
                    WHERE joining_mod_id = $1
                    ",
                    id as database::models::ids::ModId,
                )
                .execute(&mut *transaction)
                .await
                .map_err(|e| ApiError::DatabaseError(e.into()))?;

                for donation in donations {
                    let platform_id = database::models::DonationPlatformId::get_id(
                        &donation.id,
                        &mut *transaction,
                    )
                    .await?
                    .ok_or_else(|| {
                        ApiError::InvalidInputError(format!(
                            "Platform {} does not exist.",
                            donation.id.clone()
                        ))
                    })?;

                    sqlx::query!(
                        "
                        INSERT INTO mods_donations (joining_mod_id, joining_platform_id, url)
                        VALUES ($1, $2, $3)
                        ",
                        id as database::models::ids::ModId,
                        platform_id as database::models::ids::DonationPlatformId,
                        donation.url
                    )
                    .execute(&mut *transaction)
                    .await
                    .map_err(|e| ApiError::DatabaseError(e.into()))?;
                }
            }

            if let Some(body) = &new_mod.body {
                if !perms.contains(Permissions::EDIT_BODY) {
                    return Err(ApiError::CustomAuthenticationError(
                        "You do not have the permissions to edit the body of this mod!".to_string(),
                    ));
                }

                if body.len() > 65536 {
                    return Err(ApiError::InvalidInputError(
                        "The mod's body must be less than 65536 characters!".to_string(),
                    ));
                }

                sqlx::query!(
                    "
                    UPDATE mods
                    SET body = $1
                    WHERE (id = $2)
                    ",
                    body,
                    id as database::models::ids::ModId,
                )
                .execute(&mut *transaction)
                .await
                .map_err(|e| ApiError::DatabaseError(e.into()))?;
            }


            if let Some(is_nsfw) = &new_mod.is_nsfw {
                if !perms.contains(Permissions::EDIT_DETAILS) {
                    return Err(ApiError::CustomAuthenticationError(
                        "You do not have the permissions to edit the sfw status of this mod!".to_string(),
                    ));
                }

                sqlx::query!(
                    "
                    UPDATE mods
                    SET is_nsfw = $1
                    WHERE (id = $2)
                    ",
                    is_nsfw,
                    id as database::models::ids::ModId,
                )
                .execute(&mut *transaction)
                .await
                .map_err(|e| ApiError::DatabaseError(e.into()))?;
            }

            transaction
                .commit()
                .await
                .map_err(|e| ApiError::DatabaseError(e.into()))?;
            Ok(HttpResponse::Ok().body(""))
        } else {
            Err(ApiError::CustomAuthenticationError(
                "You do not have permission to edit this mod!".to_string(),
            ))
        }
    } else {
        Ok(HttpResponse::NotFound().body(""))
    }
}

#[derive(Serialize, Deserialize)]
pub struct Extension {
    pub ext: String,
}

#[patch("{id}/icon")]
pub async fn mod_icon_edit(
    web::Query(ext): web::Query<Extension>,
    req: HttpRequest,
    info: web::Path<(models::ids::ModId,)>,
    pool: web::Data<PgPool>,
    file_host: web::Data<Arc<dyn FileHost + Send + Sync>>,
    mut payload: web::Payload,
) -> Result<HttpResponse, ApiError> {
    if let Some(content_type) = super::mod_creation::get_image_content_type(&*ext.ext) {
        let cdn_url = dotenv::var("CDN_URL")?;
        let user = get_user_from_headers(req.headers(), &**pool).await?;
        let id = info.into_inner().0;

        let mod_item = database::models::Mod::get(id.into(), &**pool)
            .await
            .map_err(|e| ApiError::DatabaseError(e.into()))?
            .ok_or_else(|| ApiError::InvalidInputError("Invalid Mod ID specified!".to_string()))?;

        if !user.role.is_mod() {
            let team_member = database::models::TeamMember::get_from_user_id(
                mod_item.team_id,
                user.id.into(),
                &**pool,
            )
            .await
            .map_err(ApiError::DatabaseError)?
            .ok_or_else(|| ApiError::InvalidInputError("Invalid Mod ID specified!".to_string()))?;

            if !team_member.permissions.contains(Permissions::EDIT_DETAILS) {
                return Err(ApiError::CustomAuthenticationError(
                    "You don't have permission to edit this mod's icon.".to_string(),
                ));
            }
        }

        if let Some(icon) = mod_item.icon_url {
            let name = icon.split('/').next();

            if let Some(icon_path) = name {
                file_host.delete_file_version("", icon_path).await?;
            }
        }

        let mut bytes = web::BytesMut::new();
        while let Some(item) = payload.next().await {
            bytes.extend_from_slice(&item.map_err(|_| {
                ApiError::InvalidInputError("Unable to parse bytes in payload sent!".to_string())
            })?);
        }

        if bytes.len() >= 262144 {
            return Err(ApiError::InvalidInputError(String::from(
                "Icons must be smaller than 256KiB",
            )));
        }

        let upload_data = file_host
            .upload_file(
                content_type,
                &format!("data/{}/icon.{}", id, ext.ext),
                bytes.to_vec(),
            )
            .await?;

        let mod_id: database::models::ids::ModId = id.into();
        sqlx::query!(
            "
            UPDATE mods
            SET icon_url = $1
            WHERE (id = $2)
            ",
            format!("{}/{}", cdn_url, upload_data.file_name),
            mod_id as database::models::ids::ModId,
        )
        .execute(&**pool)
        .await
        .map_err(|e| ApiError::DatabaseError(e.into()))?;

        Ok(HttpResponse::Ok().body(""))
    } else {
        Err(ApiError::InvalidInputError(format!(
            "Invalid format for mod icon: {}",
            ext.ext
        )))
    }
}

#[delete("{id}")]
pub async fn mod_delete(
    req: HttpRequest,
    info: web::Path<(models::ids::ModId,)>,
    pool: web::Data<PgPool>,
    config: web::Data<SearchConfig>,
) -> Result<HttpResponse, ApiError> {
    let user = get_user_from_headers(req.headers(), &**pool).await?;
    let id = info.into_inner().0;

    if !user.role.is_mod() {
        let team_member =
            database::models::TeamMember::get_from_user_id_mod(id.into(), user.id.into(), &**pool)
                .await
                .map_err(ApiError::DatabaseError)?
                .ok_or_else(|| {
                    ApiError::InvalidInputError("Invalid Mod ID specified!".to_string())
                })?;

        if !team_member.permissions.contains(Permissions::DELETE_MOD) {
            return Err(ApiError::CustomAuthenticationError(
                "You don't have permission to delete this mod".to_string(),
            ));
        }
    }

    let result = database::models::Mod::remove_full(id.into(), &**pool)
        .await
        .map_err(|e| ApiError::DatabaseError(e.into()))?;

    delete_from_index(id, config).await?;

    if result.is_some() {
        Ok(HttpResponse::Ok().body(""))
    } else {
        Ok(HttpResponse::NotFound().body(""))
    }
}

#[post("{id}/follow")]
pub async fn mod_follow(
    req: HttpRequest,
    info: web::Path<(models::ids::ModId,)>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ApiError> {
    let user = get_user_from_headers(req.headers(), &**pool).await?;
    let id = info.into_inner().0;

    let _result = database::models::Mod::get(id.into(), &**pool)
        .await
        .map_err(|e| ApiError::DatabaseError(e.into()))?
        .ok_or_else(|| ApiError::InvalidInputError("Invalid Mod ID specified!".to_string()))?;

    let user_id: database::models::ids::UserId = user.id.into();
    let mod_id: database::models::ids::ModId = id.into();

    let following = sqlx::query!(
        "
        SELECT EXISTS(SELECT 1 FROM mod_follows mf WHERE mf.follower_id = $1 AND mf.mod_id = $2)
        ",
        user_id as database::models::ids::UserId,
        mod_id as database::models::ids::ModId
    )
    .fetch_one(&**pool)
    .await
    .map_err(|e| ApiError::DatabaseError(e.into()))?
    .exists
    .unwrap_or(false);

    if !following {
        sqlx::query!(
            "
            UPDATE mods
            SET follows = follows + 1
            WHERE id = $1
            ",
            mod_id as database::models::ids::ModId,
        )
        .execute(&**pool)
        .await
        .map_err(|e| ApiError::DatabaseError(e.into()))?;

        sqlx::query!(
            "
            INSERT INTO mod_follows (follower_id, mod_id)
            VALUES ($1, $2)
            ",
            user_id as database::models::ids::UserId,
            mod_id as database::models::ids::ModId
        )
        .execute(&**pool)
        .await
        .map_err(|e| ApiError::DatabaseError(e.into()))?;

        Ok(HttpResponse::Ok().body(""))
    } else {
        Err(ApiError::InvalidInputError(
            "You are already following this mod!".to_string(),
        ))
    }
}

#[delete("{id}/follow")]
pub async fn mod_unfollow(
    req: HttpRequest,
    info: web::Path<(models::ids::ModId,)>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ApiError> {
    let user = get_user_from_headers(req.headers(), &**pool).await?;
    let id = info.into_inner().0;

    let user_id: database::models::ids::UserId = user.id.into();
    let mod_id: database::models::ids::ModId = id.into();

    let following = sqlx::query!(
        "
        SELECT EXISTS(SELECT 1 FROM mod_follows mf WHERE mf.follower_id = $1 AND mf.mod_id = $2)
        ",
        user_id as database::models::ids::UserId,
        mod_id as database::models::ids::ModId
    )
    .fetch_one(&**pool)
    .await
    .map_err(|e| ApiError::DatabaseError(e.into()))?
    .exists
    .unwrap_or(false);

    if following {
        sqlx::query!(
            "
            UPDATE mods
            SET follows = follows - 1
            WHERE id = $1
            ",
            mod_id as database::models::ids::ModId,
        )
        .execute(&**pool)
        .await
        .map_err(|e| ApiError::DatabaseError(e.into()))?;

        sqlx::query!(
            "
            DELETE FROM mod_follows
            WHERE follower_id = $1 AND mod_id = $2
            ",
            user_id as database::models::ids::UserId,
            mod_id as database::models::ids::ModId
        )
        .execute(&**pool)
        .await
        .map_err(|e| ApiError::DatabaseError(e.into()))?;

        Ok(HttpResponse::Ok().body(""))
    } else {
        Err(ApiError::InvalidInputError(
            "You are not following this mod!".to_string(),
        ))
    }
}

pub async fn delete_from_index(
    id: crate::models::mods::ModId,
    config: web::Data<SearchConfig>,
) -> Result<(), meilisearch_sdk::errors::Error> {
    let client = meilisearch_sdk::client::Client::new(&*config.address, &*config.key);

    let indexes: Vec<meilisearch_sdk::indexes::Index> = client.get_indexes().await?;
    for index in indexes {
        index.delete_document(format!("local-{}", id)).await?;
    }

    Ok(())
}
