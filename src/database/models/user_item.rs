use super::ids::{ModId, UserId};

pub struct User {
    pub id: UserId,
    pub discord_id: Option<String>,
    pub username: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub created: chrono::DateTime<chrono::Utc>,
    pub role: String,
    pub show_nsfw: bool,
}

impl User {
    pub async fn insert(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<(), sqlx::error::Error> {
        sqlx::query!(
            "
            INSERT INTO users (
                id, discord_id, username, name, email,
                avatar_url, bio, created
            )
            VALUES (
                $1, $2, LOWER($3), $4, $5,
                $6, $7, $8
            )
            ",
            self.id as UserId,
            self.discord_id,
            &self.username,
            self.name.as_ref(),
            self.email.as_ref(),
            self.avatar_url.as_ref(),
            self.bio.as_ref(),
            self.created,
        )
        .execute(&mut *transaction)
        .await?;

        Ok(())
    }
    pub async fn get<'a, 'b, E>(id: UserId, executor: E) -> Result<Option<Self>, sqlx::error::Error>
    where
        E: sqlx::Executor<'a, Database = sqlx::Postgres>,
    {
        let result = sqlx::query!(
            "
            SELECT u.discord_id, u.name, u.email,
                u.avatar_url, u.username, u.bio,
                u.created, u.role, u.show_nsfw
            FROM users u
            WHERE u.id = $1
            ",
            id as UserId,
        )
        .fetch_optional(executor)
        .await?;

        if let Some(row) = result {
            Ok(Some(User {
                id,
                discord_id: row.discord_id,
                name: row.name,
                email: row.email,
                avatar_url: row.avatar_url,
                username: row.username,
                bio: row.bio,
                created: row.created,
                role: row.role,
                show_nsfw: row.show_nsfw,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_from_discord_id<'a, 'b, E>(
        discord_id: String,
        executor: E,
    ) -> Result<Option<Self>, sqlx::error::Error>
    where
        E: sqlx::Executor<'a, Database = sqlx::Postgres>,
    {
        let result = sqlx::query!(
            "
            SELECT u.id, u.name, u.email,
                u.avatar_url, u.username, u.bio,
                u.created, u.role, u.show_nsfw
            FROM users u
            WHERE u.discord_id = $1
            ",
            discord_id,
        )
        .fetch_optional(executor)
        .await?;

        if let Some(row) = result {
            Ok(Some(User {
                id: UserId(row.id),
                discord_id: Some(discord_id),
                name: row.name,
                email: row.email,
                avatar_url: row.avatar_url,
                username: row.username,
                bio: row.bio,
                created: row.created,
                role: row.role,
                show_nsfw: row.show_nsfw,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_from_username<'a, 'b, E>(
        username: String,
        executor: E,
    ) -> Result<Option<Self>, sqlx::error::Error>
    where
        E: sqlx::Executor<'a, Database = sqlx::Postgres>,
    {
        let result = sqlx::query!(
            "
            SELECT u.id, u.discord_id, u.name, u.email,
                u.avatar_url, u.bio,
                u.created, u.role, u.show_nsfw
            FROM users u
            WHERE LOWER(u.username) = LOWER($1)
            ",
            username
        )
        .fetch_optional(executor)
        .await?;

        if let Some(row) = result {
            Ok(Some(User {
                id: UserId(row.id),
                discord_id: row.discord_id,
                name: row.name,
                email: row.email,
                avatar_url: row.avatar_url,
                username,
                bio: row.bio,
                created: row.created,
                role: row.role,
                show_nsfw: row.show_nsfw,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_many<'a, E>(user_ids: Vec<UserId>, exec: E) -> Result<Vec<User>, sqlx::Error>
    where
        E: sqlx::Executor<'a, Database = sqlx::Postgres> + Copy,
    {
        use futures::stream::TryStreamExt;

        let user_ids_parsed: Vec<i64> = user_ids.into_iter().map(|x| x.0).collect();
        let users = sqlx::query!(
            "
            SELECT u.id, u.discord_id, u.name, u.email,
                u.avatar_url, u.username, u.bio,
                u.created, u.role, u.show_nsfw FROM users u
            WHERE u.id IN (SELECT * FROM UNNEST($1::bigint[]))
            ",
            &user_ids_parsed
        )
        .fetch_many(exec)
        .try_filter_map(|e| async {
            Ok(e.right().map(|u| User {
                id: UserId(u.id),
                discord_id: u.discord_id,
                name: u.name,
                email: u.email,
                avatar_url: u.avatar_url,
                username: u.username,
                bio: u.bio,
                created: u.created,
                role: u.role,
                show_nsfw: u.show_nsfw
            }))
        })
        .try_collect::<Vec<User>>()
        .await?;

        Ok(users)
    }

    pub async fn get_mods<'a, E>(
        user_id: UserId,
        status: &str,
        exec: E,
    ) -> Result<Vec<ModId>, sqlx::Error>
    where
        E: sqlx::Executor<'a, Database = sqlx::Postgres> + Copy,
    {
        use futures::stream::TryStreamExt;

        let mods = sqlx::query!(
            "
            SELECT m.id FROM mods m
            INNER JOIN team_members tm ON tm.team_id = m.team_id
            WHERE tm.user_id = $1 AND m.status = (SELECT s.id FROM statuses s WHERE s.status = $2)
            ",
            user_id as UserId,
            status,
        )
        .fetch_many(exec)
        .try_filter_map(|e| async { Ok(e.right().map(|m| ModId(m.id))) })
        .try_collect::<Vec<ModId>>()
        .await?;

        Ok(mods)
    }

    pub async fn get_mods_private<'a, E>(
        user_id: UserId,
        exec: E,
    ) -> Result<Vec<ModId>, sqlx::Error>
    where
        E: sqlx::Executor<'a, Database = sqlx::Postgres> + Copy,
    {
        use futures::stream::TryStreamExt;

        let mods = sqlx::query!(
            "
            SELECT m.id FROM mods m
            INNER JOIN team_members tm ON tm.team_id = m.team_id
            WHERE tm.user_id = $1
            ",
            user_id as UserId,
        )
        .fetch_many(exec)
        .try_filter_map(|e| async { Ok(e.right().map(|m| ModId(m.id))) })
        .try_collect::<Vec<ModId>>()
        .await?;

        Ok(mods)
    }

    pub async fn remove<'a, 'b, E>(id: UserId, exec: E) -> Result<Option<()>, sqlx::error::Error>
    where
        E: sqlx::Executor<'a, Database = sqlx::Postgres> + Copy,
    {
        let deleted_user: UserId = crate::models::users::DELETED_USER.into();

        sqlx::query!(
            "
            UPDATE team_members
            SET user_id = $1
            WHERE (user_id = $2 AND role = $3)
            ",
            deleted_user as UserId,
            id as UserId,
            crate::models::teams::OWNER_ROLE
        )
        .execute(exec)
        .await?;

        sqlx::query!(
            "
            UPDATE versions
            SET author_id = $1
            WHERE (author_id = $2)
            ",
            deleted_user as UserId,
            id as UserId,
        )
        .execute(exec)
        .await?;

        use futures::TryStreamExt;
        let notifications: Vec<i64> = sqlx::query!(
            "
            SELECT n.id FROM notifications n
            WHERE n.user_id = $1
            ",
            id as UserId,
        )
        .fetch_many(exec)
        .try_filter_map(|e| async { Ok(e.right().map(|m| m.id as i64)) })
        .try_collect::<Vec<i64>>()
        .await?;

        sqlx::query!(
            "
            DELETE FROM notifications
            WHERE user_id = $1
            ",
            id as UserId,
        )
        .execute(exec)
        .await?;

        sqlx::query!(
            "
            DELETE FROM reports
            WHERE user_id = $1
            ",
            id as UserId,
        )
        .execute(exec)
        .await?;

        sqlx::query!(
            "
            DELETE FROM mod_follows
            WHERE follower_id = $1
            ",
            id as UserId,
        )
        .execute(exec)
        .await?;

        sqlx::query!(
            "
            DELETE FROM notifications_actions
             WHERE notification_id IN (SELECT * FROM UNNEST($1::bigint[]))
            ",
            &notifications
        )
        .execute(exec)
        .await?;

        sqlx::query!(
            "
            DELETE FROM team_members
            WHERE user_id = $1
            ",
            id as UserId,
        )
        .execute(exec)
        .await?;

        sqlx::query!(
            "
            DELETE FROM users
            WHERE id = $1
            ",
            id as UserId,
        )
        .execute(exec)
        .await?;

        Ok(Some(()))
    }

    pub async fn remove_full<'a, 'b, E>(
        id: UserId,
        exec: E,
    ) -> Result<Option<()>, sqlx::error::Error>
    where
        E: sqlx::Executor<'a, Database = sqlx::Postgres> + Copy,
    {
        use futures::TryStreamExt;
        let mods: Vec<ModId> = sqlx::query!(
            "
            SELECT m.id FROM mods m
            INNER JOIN team_members tm ON tm.team_id = m.team_id
            WHERE tm.user_id = $1 AND tm.role = $2
            ",
            id as UserId,
            crate::models::teams::OWNER_ROLE
        )
        .fetch_many(exec)
        .try_filter_map(|e| async { Ok(e.right().map(|m| ModId(m.id))) })
        .try_collect::<Vec<ModId>>()
        .await?;

        for mod_id in mods {
            let _result = super::mod_item::Mod::remove_full(mod_id, exec).await?;
        }

        let notifications: Vec<i64> = sqlx::query!(
            "
            SELECT n.id FROM notifications n
            WHERE n.user_id = $1
            ",
            id as UserId,
        )
        .fetch_many(exec)
        .try_filter_map(|e| async { Ok(e.right().map(|m| m.id as i64)) })
        .try_collect::<Vec<i64>>()
        .await?;

        sqlx::query!(
            "
            DELETE FROM notifications
            WHERE user_id = $1
            ",
            id as UserId,
        )
        .execute(exec)
        .await?;

        sqlx::query!(
            "
            DELETE FROM notifications_actions
             WHERE notification_id IN (SELECT * FROM UNNEST($1::bigint[]))
            ",
            &notifications
        )
        .execute(exec)
        .await?;

        let deleted_user: UserId = crate::models::users::DELETED_USER.into();

        sqlx::query!(
            "
            UPDATE versions
            SET author_id = $1
            WHERE (author_id = $2)
            ",
            deleted_user as UserId,
            id as UserId,
        )
        .execute(exec)
        .await?;

        sqlx::query!(
            "
            DELETE FROM team_members
            WHERE user_id = $1
            ",
            id as UserId,
        )
        .execute(exec)
        .await?;

        sqlx::query!(
            "
            DELETE FROM users
            WHERE id = $1
            ",
            id as UserId,
        )
        .execute(exec)
        .await?;

        Ok(Some(()))
    }
}
