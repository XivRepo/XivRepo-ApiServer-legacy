use super::ids::*;
use crate::models::ids::base62_impl::parse_base62;
use crate::models::mods::Dependency;

#[derive(Debug)]
pub struct DonationUrl {
    pub mod_id: ModId,
    pub platform_id: DonationPlatformId,
    pub platform_short: String,
    pub platform_name: String,
    pub url: String,
}

impl DonationUrl {
    pub async fn insert(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<(), sqlx::error::Error> {
        sqlx::query!(
            "
            INSERT INTO mods_donations (
                joining_mod_id, joining_platform_id, url
            )
            VALUES (
                $1, $2, $3
            )
            ",
            self.mod_id as ModId,
            self.platform_id as DonationPlatformId,
            self.url,
        )
        .execute(&mut *transaction)
        .await?;

        Ok(())
    }
}
#[derive(Debug)]
pub struct ModBuilder {
    pub mod_id: ModId,
    pub team_id: TeamId,
    pub title: String,
    pub description: String,
    pub body: String,
    pub icon_url: Option<String>,
    pub issues_url: Option<String>,
    pub source_url: Option<String>,
    pub wiki_url: Option<String>,
    pub discord_url: Option<String>,
    pub categories: Vec<CategoryId>,
    pub initial_versions: Vec<super::version_item::VersionBuilder>,
    pub status: StatusId,
    pub is_nsfw: bool,
    pub slug: String,
    pub donation_urls: Vec<DonationUrl>,
    pub dependencies: Option<Vec<Dependency>>,
}

impl ModBuilder {
    pub async fn insert(
        self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<ModId, super::DatabaseError> {
        let mod_struct = Mod {
            id: self.mod_id,
            team_id: self.team_id,
            title: self.title,
            description: self.description,
            body: self.body,
            body_url: None,
            published: chrono::Utc::now(),
            updated: chrono::Utc::now(),
            status: self.status,
            is_nsfw: self.is_nsfw,
            downloads: 0,
            follows: 0,
            icon_url: self.icon_url,
            issues_url: self.issues_url,
            source_url: self.source_url,
            wiki_url: self.wiki_url,
            discord_url: self.discord_url,
            slug: Some(self.slug),
        };
        mod_struct.insert(&mut *transaction).await?; // inserts all of the above values into the DB

        for mut version in self.initial_versions {
            version.mod_id = self.mod_id;
            version.insert(&mut *transaction).await?;
        }

        for mut donation in self.donation_urls {
            donation.mod_id = self.mod_id;
            donation.insert(&mut *transaction).await?;
        }

        for category in self.categories {
            sqlx::query!(
                "
                INSERT INTO mods_categories (joining_mod_id, joining_category_id)
                VALUES ($1, $2)
                ",
                self.mod_id as ModId,
                category as CategoryId,
            )
            .execute(&mut *transaction)
            .await?;
        }

        if let Some(deps) = self.dependencies {
            for dep in deps {
                let b62 = parse_base62(dep.mod_id.0.to_string().as_str()).unwrap() as i64;
                let table_id = generate_dependency_id(&mut *transaction).await?;
                sqlx::query!(
                    "
                    INSERT INTO dependencies (id, dependency_type, dependent_id, dependency_id)
                    VALUES ($1, $2, $3, $4)
                    ",
                    table_id.0 as i64,
                    crate::models::mods::DependencyType::Required.as_str(),
                    self.mod_id.0,
                    b62,
                )
                .execute(&mut *transaction)
                .await?;
            }
        }

        Ok(self.mod_id)
    }
}

pub struct Mod {
    pub id: ModId,
    pub team_id: TeamId,
    pub title: String,
    pub description: String,
    pub body: String,
    pub body_url: Option<String>,
    pub published: chrono::DateTime<chrono::Utc>,
    pub updated: chrono::DateTime<chrono::Utc>,
    pub status: StatusId,
    pub is_nsfw: bool,
    pub downloads: i32,
    pub follows: i32,
    pub icon_url: Option<String>,
    pub issues_url: Option<String>,
    pub source_url: Option<String>,
    pub wiki_url: Option<String>,
    pub discord_url: Option<String>,
    pub slug: Option<String>,
}

impl Mod {
    pub async fn insert(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<(), sqlx::error::Error> {
        sqlx::query!(
            "
            INSERT INTO mods (
                id, team_id, title, description, body,
                published, downloads, icon_url, issues_url,
                source_url, wiki_url, status, discord_url,
                slug, is_nsfw
            )
            VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9,
                $10, $11, $12, $13,
                LOWER($14), $15
            )
            ",
            self.id as ModId,
            self.team_id as TeamId,
            &self.title,
            &self.description,
            &self.body,
            self.published,
            self.downloads,
            self.icon_url.as_ref(),
            self.issues_url.as_ref(),
            self.source_url.as_ref(),
            self.wiki_url.as_ref(),
            self.status.0,
            self.discord_url.as_ref(),
            self.slug.as_ref(),
            self.is_nsfw,
        )
        .execute(&mut *transaction)
        .await?;

        Ok(())
    }

    pub async fn get<'a, 'b, E>(id: ModId, executor: E) -> Result<Option<Self>, sqlx::error::Error>
    where
        E: sqlx::Executor<'a, Database = sqlx::Postgres>,
    {
        let result = sqlx::query!(
            "
            SELECT title, description, downloads, follows,
                   icon_url, body, body_url, published,
                   updated, status, is_nsfw,
                   issues_url, source_url, wiki_url, discord_url,
                   team_id, slug
            FROM mods
            WHERE id = $1
            ",
            id as ModId,
        )
        .fetch_optional(executor)
        .await?;

        if let Some(row) = result {
            Ok(Some(Mod {
                id,
                team_id: TeamId(row.team_id),
                title: row.title,
                description: row.description,
                downloads: row.downloads,
                body_url: row.body_url,
                icon_url: row.icon_url,
                published: row.published,
                updated: row.updated,
                issues_url: row.issues_url,
                source_url: row.source_url,
                wiki_url: row.wiki_url,
                discord_url: row.discord_url,
                status: StatusId(row.status),
                slug: row.slug,
                body: row.body,
                follows: row.follows,
                is_nsfw: row.is_nsfw,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_many<'a, E>(mod_ids: Vec<ModId>, exec: E) -> Result<Vec<Mod>, sqlx::Error>
    where
        E: sqlx::Executor<'a, Database = sqlx::Postgres> + Copy,
    {
        use futures::stream::TryStreamExt;

        let mod_ids_parsed: Vec<i64> = mod_ids.into_iter().map(|x| x.0).collect();
        let mods = sqlx::query!(
            "
            SELECT id, title, description, downloads, follows,
                   icon_url, body, body_url, published,
                   updated, status, is_nsfw,
                   issues_url, source_url, wiki_url, discord_url,
                   team_id, slug
            FROM mods
            WHERE id IN (SELECT * FROM UNNEST($1::bigint[]))
            ",
            &mod_ids_parsed
        )
        .fetch_many(exec)
        .try_filter_map(|e| async {
            Ok(e.right().map(|m| Mod {
                id: ModId(m.id),
                team_id: TeamId(m.team_id),
                title: m.title,
                description: m.description,
                downloads: m.downloads,
                body_url: m.body_url,
                icon_url: m.icon_url,
                published: m.published,
                updated: m.updated,
                issues_url: m.issues_url,
                source_url: m.source_url,
                wiki_url: m.wiki_url,
                discord_url: m.discord_url,
                status: StatusId(m.status),
                is_nsfw: m.is_nsfw,
                slug: m.slug,
                body: m.body,
                follows: m.follows,
            }))
        })
        .try_collect::<Vec<Mod>>()
        .await?;

        Ok(mods)
    }

    pub async fn remove_full<'a, 'b, E>(
        id: ModId,
        exec: E,
    ) -> Result<Option<()>, sqlx::error::Error>
    where
        E: sqlx::Executor<'a, Database = sqlx::Postgres> + Copy,
    {
        let result = sqlx::query!(
            "
            SELECT team_id FROM mods WHERE id = $1
            ",
            id as ModId,
        )
        .fetch_optional(exec)
        .await?;

        let team_id: TeamId = if let Some(id) = result {
            TeamId(id.team_id)
        } else {
            return Ok(None);
        };

        sqlx::query!(
            "
            DELETE FROM mod_follows
            WHERE mod_id = $1
            ",
            id as ModId
        )
        .execute(exec)
        .await?;

        sqlx::query!(
            "
            DELETE FROM mod_follows
            WHERE mod_id = $1
            ",
            id as ModId,
        )
        .execute(exec)
        .await?;

        sqlx::query!(
            "
            DELETE FROM reports
            WHERE mod_id = $1
            ",
            id as ModId,
        )
        .execute(exec)
        .await?;

        sqlx::query!(
            "
            DELETE FROM mods_categories
            WHERE joining_mod_id = $1
            ",
            id as ModId,
        )
        .execute(exec)
        .await?;

        sqlx::query!(
            "
            DELETE FROM mods_donations
            WHERE joining_mod_id = $1
            ",
            id as ModId,
        )
        .execute(exec)
        .await?;

        use futures::TryStreamExt;
        let versions: Vec<VersionId> = sqlx::query!(
            "
            SELECT id FROM versions
            WHERE mod_id = $1
            ",
            id as ModId,
        )
        .fetch_many(exec)
        .try_filter_map(|e| async { Ok(e.right().map(|c| VersionId(c.id))) })
        .try_collect::<Vec<VersionId>>()
        .await?;

        for version in versions {
            super::Version::remove_full(version, exec).await?;
        }

        sqlx::query!(
            "
            DELETE FROM mods
            WHERE id = $1
            ",
            id as ModId,
        )
        .execute(exec)
        .await?;

        sqlx::query!(
            "
            DELETE FROM team_members
            WHERE team_id = $1
            ",
            team_id as TeamId,
        )
        .execute(exec)
        .await?;

        sqlx::query!(
            "
            DELETE FROM teams
            WHERE id = $1
            ",
            team_id as TeamId,
        )
        .execute(exec)
        .await?;

        sqlx::query!(
            "
            DELETE FROM dependencies
            WHERE dependent_id = $1 OR dependency_id = $1
            ",
            id as ModId,
        )
        .execute(exec)
        .await?;

        Ok(Some(()))
    }

    pub async fn get_full_from_slug<'a, 'b, E>(
        slug: &str,
        executor: E,
    ) -> Result<Option<QueryMod>, sqlx::error::Error>
    where
        E: sqlx::Executor<'a, Database = sqlx::Postgres> + Copy,
    {
        let id = sqlx::query!(
            "
                SELECT id FROM mods
                WHERE LOWER(slug) = LOWER($1)
                ",
            slug
        )
        .fetch_optional(executor)
        .await?;

        if let Some(mod_id) = id {
            Mod::get_full(ModId(mod_id.id), executor).await
        } else {
            Ok(None)
        }
    }

    pub async fn get_full<'a, 'b, E>(
        id: ModId,
        executor: E,
    ) -> Result<Option<QueryMod>, sqlx::error::Error>
    where
        E: sqlx::Executor<'a, Database = sqlx::Postgres> + Copy,
    {
        let result = sqlx::query!(
            "
            SELECT m.id id, m.title title, m.description description, m.downloads downloads, m.follows follows,
            m.icon_url icon_url, m.body body, m.body_url body_url, m.published published, m.is_nsfw,
            m.updated updated, m.status status,
            m.issues_url issues_url, m.source_url source_url, m.wiki_url wiki_url, m.discord_url discord_url,
            m.team_id team_id, m.slug slug,
            s.status status_name,
            STRING_AGG(DISTINCT c.category, ',') categories, STRING_AGG(DISTINCT v.id::text, ',') versions,
            STRING_AGG(DISTINCT d.dependent_id::text, ',') dependencies
            FROM mods m
            LEFT OUTER JOIN mods_categories mc ON joining_mod_id = m.id
            LEFT OUTER JOIN categories c ON mc.joining_category_id = c.id
            LEFT OUTER JOIN versions v ON v.mod_id = m.id
            LEFT OUTER JOIN dependencies d ON d.dependent_id = m.id
            INNER JOIN statuses s ON s.id = m.status
            WHERE m.id = $1
            GROUP BY m.id, s.id;
            ",
            id as ModId,
        )
        .fetch_optional(executor)
        .await?;

        if let Some(m) = result {
            Ok(Some(QueryMod { // maybe refactor this? duplicated on line 556
                inner: Mod {
                    id: ModId(m.id),
                    team_id: TeamId(m.team_id),
                    title: m.title.clone(),
                    description: m.description.clone(),
                    downloads: m.downloads,
                    body_url: m.body_url.clone(),
                    icon_url: m.icon_url.clone(),
                    published: m.published,
                    updated: m.updated,
                    issues_url: m.issues_url.clone(),
                    source_url: m.source_url.clone(),
                    wiki_url: m.wiki_url.clone(),
                    discord_url: m.discord_url.clone(),
                    status: StatusId(m.status),
                    is_nsfw: m.is_nsfw,
                    slug: m.slug.clone(),
                    body: m.body.clone(),
                    follows: m.follows,
                },
                categories: m
                    .categories
                    .unwrap_or_default()
                    .split(',')
                    .map(|x| x.to_string())
                    .collect(),
                versions: m
                    .versions
                    .unwrap_or_default()
                    .split(',')
                    .map(|x| VersionId(x.parse().unwrap_or_default()))
                    .collect(),
                donation_urls: vec![],
                dependencies: m.dependencies.unwrap_or_default().split(',').map(|x| x.to_string()).collect(),
                status: crate::models::mods::ModStatus::from_str(&m.status_name),
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_many_full<'a, E>(
        mod_ids: Vec<ModId>,
        exec: E,
    ) -> Result<Vec<QueryMod>, sqlx::Error>
    where
        E: sqlx::Executor<'a, Database = sqlx::Postgres> + Copy,
    {
        use futures::TryStreamExt;

        let mod_ids_parsed: Vec<i64> = mod_ids.into_iter().map(|x| x.0).collect();
        sqlx::query!(
            "
            SELECT m.id id, m.title title, m.description description, m.downloads downloads, m.follows follows,
            m.icon_url icon_url, m.body body, m.body_url body_url, m.published published, m.is_nsfw,
            m.updated updated, m.status status,
            m.issues_url issues_url, m.source_url source_url, m.wiki_url wiki_url, m.discord_url discord_url,
            m.team_id team_id, m.slug slug,
            s.status status_name,
            STRING_AGG(DISTINCT c.category, ',') categories, STRING_AGG(DISTINCT v.id::text, ',') versions,
            STRING_AGG(DISTINCT d.dependent_id::text, ',') dependencies
            FROM mods m
            LEFT OUTER JOIN mods_categories mc ON joining_mod_id = m.id
            LEFT OUTER JOIN categories c ON mc.joining_category_id = c.id
            LEFT OUTER JOIN versions v ON v.mod_id = m.id
            LEFT OUTER JOIN dependencies d ON d.dependent_id = m.id
            INNER JOIN statuses s ON s.id = m.status
            WHERE m.id IN (SELECT * FROM UNNEST($1::bigint[]))
            GROUP BY m.id, s.id;
            ",
            &mod_ids_parsed
        )
            .fetch_many(exec)
            .try_filter_map(|e| async {
                Ok(e.right().map(|m| QueryMod { // maybe refactor this? duplicated on line 475
                    inner: Mod {
                        id: ModId(m.id),
                        team_id: TeamId(m.team_id),
                        title: m.title.clone(),
                        description: m.description.clone(),
                        downloads: m.downloads,
                        body_url: m.body_url.clone(),
                        icon_url: m.icon_url.clone(),
                        published: m.published,
                        updated: m.updated,
                        issues_url: m.issues_url.clone(),
                        source_url: m.source_url.clone(),
                        wiki_url: m.wiki_url.clone(),
                        discord_url: m.discord_url.clone(),
                        status: StatusId(m.status),
                        is_nsfw: m.is_nsfw,
                        slug: m.slug.clone(),
                        body: m.body.clone(),
                        follows: m.follows,
                    },
                    categories: m.categories.unwrap_or_default().split(',').map(|x| x.to_string()).collect(),
                    versions: m.versions.unwrap_or_default().split(',').map(|x| VersionId(x.parse().unwrap_or_default())).collect(),
                    donation_urls: vec![],
                    dependencies: m.dependencies.unwrap_or_default().split(',').map(|x| x.to_string()).collect(),
                    status: crate::models::mods::ModStatus::from_str(&m.status_name),
                }))
            })
            .try_collect::<Vec<QueryMod>>()
            .await
    }
}

pub struct QueryMod {
    pub inner: Mod,

    pub categories: Vec<String>,
    pub versions: Vec<VersionId>,
    pub donation_urls: Vec<DonationUrl>,
    pub dependencies: Vec<String>, // Returns a list of mod ids for all of its dependencies
    pub status: crate::models::mods::ModStatus,
}
