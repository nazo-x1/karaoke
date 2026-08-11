//! 歌曲仓储。对应 Python `karaoke/infra/repositories/song_repo.py`。

use crate::models::{NewSong, SongRow};
use sqlx::postgres::PgPool;
use std::collections::{HashMap, HashSet};

pub const PAGE_SIZE: i64 = 20;

#[derive(Clone)]
pub struct SongRepository {
    pool: PgPool,
}

impl SongRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, song_id: i64) -> Result<SongRow, sqlx::Error> {
        sqlx::query_as::<_, SongRow>("SELECT * FROM song WHERE id = $1")
            .bind(song_id)
            .fetch_one(&self.pool)
            .await
    }

    pub async fn get_optional(&self, song_id: i64) -> Result<Option<SongRow>, sqlx::Error> {
        sqlx::query_as::<_, SongRow>("SELECT * FROM song WHERE id = $1")
            .bind(song_id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn find_by_source_path(&self, path: &str) -> Result<Option<SongRow>, sqlx::Error> {
        sqlx::query_as::<_, SongRow>("SELECT * FROM song WHERE source_path = $1")
            .bind(path)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn find_by_display_name(&self, name: &str) -> Result<Option<SongRow>, sqlx::Error> {
        sqlx::query_as::<_, SongRow>("SELECT * FROM song WHERE display_name = $1 LIMIT 1")
            .bind(name)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn all_display_names(&self) -> Result<HashSet<String>, sqlx::Error> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT display_name FROM song")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(|(n,)| n).collect())
    }

    pub async fn create(&self, song: &NewSong) -> Result<SongRow, sqlx::Error> {
        sqlx::query_as::<_, SongRow>(
            r#"
            INSERT INTO song (
                display_name, source_path, source_origin, source_rel,
                media_kind, playback_mode, playback_source, can_queue,
                is_playable, scan_root
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING *
            "#,
        )
        .bind(&song.display_name)
        .bind(&song.source_path)
        .bind(&song.source_origin)
        .bind(&song.source_rel)
        .bind(&song.media_kind)
        .bind(&song.playback_mode)
        .bind(&song.playback_source)
        .bind(song.can_queue)
        .bind(song.is_playable)
        .bind(&song.scan_root)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn update_display_name(
        &self,
        id: i64,
        display_name: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE song SET display_name = $1, update_time = now() WHERE id = $2")
            .bind(display_name)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_audio_layout(
        &self,
        id: i64,
        layout: &karaoke_domain::AudioLayout,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE song SET audio_layout = $1, update_time = now() WHERE id = $2")
            .bind(sqlx::types::Json(layout))
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_playback_meta(
        &self,
        id: i64,
        playback_mode: &str,
        playback_source: Option<&str>,
        can_queue: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE song SET playback_mode = $1, playback_source = $2, can_queue = $3, update_time = now() WHERE id = $4",
        )
        .bind(playback_mode)
        .bind(playback_source)
        .bind(can_queue)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_playable_flags(
        &self,
        id: i64,
        is_playable: bool,
        can_queue: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE song SET is_playable = $1, can_queue = $2, update_time = now() WHERE id = $3",
        )
        .bind(is_playable)
        .bind(can_queue)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_upload_fields(
        &self,
        id: i64,
        display_name: &str,
        is_playable: bool,
        source_origin: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE song SET display_name = $1, is_playable = $2, source_origin = $3, update_time = now() WHERE id = $4",
        )
        .bind(display_name)
        .bind(is_playable)
        .bind(source_origin)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn overwrite_source(
        &self,
        id: i64,
        source_path: &str,
        display_name: &str,
        is_playable: bool,
        source_origin: &str,
        source_rel: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE song SET source_path = $1, display_name = $2, is_playable = $3,
               source_origin = $4, source_rel = $5, update_time = now() WHERE id = $6"#,
        )
        .bind(source_path)
        .bind(display_name)
        .bind(is_playable)
        .bind(source_origin)
        .bind(source_rel)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM song WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_page(
        &self,
        q: &str,
        page: i64,
        page_size: i64,
    ) -> Result<(Vec<SongRow>, i64), sqlx::Error> {
        let page = page.max(1);
        let page_size = page_size.clamp(1, 100);
        let offset = (page - 1) * page_size;

        let (total,): (i64,) = if q.is_empty() {
            sqlx::query_as("SELECT COUNT(*) FROM song")
                .fetch_one(&self.pool)
                .await?
        } else {
            sqlx::query_as("SELECT COUNT(*) FROM song WHERE display_name ILIKE $1")
                .bind(format!("%{q}%"))
                .fetch_one(&self.pool)
                .await?
        };

        let songs = if q.is_empty() {
            sqlx::query_as::<_, SongRow>("SELECT * FROM song ORDER BY id DESC OFFSET $1 LIMIT $2")
                .bind(offset)
                .bind(page_size)
                .fetch_all(&self.pool)
                .await?
        } else {
            sqlx::query_as::<_, SongRow>(
                "SELECT * FROM song WHERE display_name ILIKE $1 ORDER BY id DESC OFFSET $2 LIMIT $3",
            )
            .bind(format!("%{q}%"))
            .bind(offset)
            .bind(page_size)
            .fetch_all(&self.pool)
            .await?
        };

        Ok((songs, total))
    }

    pub async fn map_by_ids(&self, ids: &[i64]) -> Result<HashMap<i64, SongRow>, sqlx::Error> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let songs = sqlx::query_as::<_, SongRow>("SELECT * FROM song WHERE id = ANY($1)")
            .bind(ids)
            .fetch_all(&self.pool)
            .await?;
        Ok(songs.into_iter().map(|s| (s.id, s)).collect())
    }

    /// 扫描导入命中重复文件/重复名时的统一覆盖更新（对应 Python scanner 的
    /// overwrite 分支：路径/展示名/来源目录变化，回退 playback 相关字段为 plain）。
    pub async fn apply_scan_overwrite(
        &self,
        id: i64,
        source_path: &str,
        display_name: &str,
        source_rel: Option<&str>,
        is_playable: bool,
        scan_root: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE song SET source_path = $1, display_name = $2, source_rel = $3,
               is_playable = $4, scan_root = $5, playback_mode = 'plain',
               playback_source = 'plain', can_queue = $4, source_origin = 'scan',
               update_time = now()
               WHERE id = $6"#,
        )
        .bind(source_path)
        .bind(display_name)
        .bind(source_rel)
        .bind(is_playable)
        .bind(scan_root)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn all_for_scan(&self) -> Result<Vec<SongRow>, sqlx::Error> {
        sqlx::query_as::<_, SongRow>("SELECT * FROM song")
            .fetch_all(&self.pool)
            .await
    }

    pub async fn bulk_insert(&self, songs: &[NewSong]) -> Result<(), sqlx::Error> {
        for chunk in songs.chunks(500) {
            let mut tx = self.pool.begin().await?;
            for song in chunk {
                sqlx::query(
                    r#"
                    INSERT INTO song (
                        display_name, source_path, source_origin, source_rel,
                        media_kind, playback_mode, playback_source, can_queue,
                        is_playable, scan_root
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                    "#,
                )
                .bind(&song.display_name)
                .bind(&song.source_path)
                .bind(&song.source_origin)
                .bind(&song.source_rel)
                .bind(&song.media_kind)
                .bind(&song.playback_mode)
                .bind(&song.playback_source)
                .bind(song.can_queue)
                .bind(song.is_playable)
                .bind(&song.scan_root)
                .execute(&mut *tx)
                .await?;
            }
            tx.commit().await?;
        }
        Ok(())
    }
}
