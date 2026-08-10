//! 队列/历史仓储。对应 Python `karaoke/infra/repositories/history_repo.py`。

use crate::models::HistoryRow;
use karaoke_domain::{sort_pending, QueueSortItem, QueueState};
use sqlx::postgres::PgPool;

pub const PAGE_SIZE: i64 = 20;

#[derive(Clone)]
pub struct HistoryRepository {
    pool: PgPool,
}

impl HistoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, song_id: i64) -> Result<HistoryRow, sqlx::Error> {
        sqlx::query_as::<_, HistoryRow>("SELECT * FROM history WHERE id = $1")
            .bind(song_id)
            .fetch_one(&self.pool)
            .await
    }

    pub async fn get_optional(&self, song_id: i64) -> Result<Option<HistoryRow>, sqlx::Error> {
        sqlx::query_as::<_, HistoryRow>("SELECT * FROM history WHERE id = $1")
            .bind(song_id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn create(
        &self,
        id: i64,
        name: &str,
        is_sing: i32,
        is_top: i32,
    ) -> Result<HistoryRow, sqlx::Error> {
        sqlx::query_as::<_, HistoryRow>(
            "INSERT INTO history (id, name, is_sing, is_top) VALUES ($1, $2, $3, $4) RETURNING *",
        )
        .bind(id)
        .bind(name)
        .bind(is_sing)
        .bind(is_top)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn set_pending(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE history SET is_sing = 0, is_top = 0, update_time = now() WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_top(&self, id: i64) -> Result<u64, sqlx::Error> {
        let result =
            sqlx::query("UPDATE history SET is_top = 1, update_time = now() WHERE id = $1")
                .bind(id)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected())
    }

    pub async fn mark_singing(&self, id: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE history SET is_sing = -1, is_top = 0, update_time = now() WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn mark_finished(&self, id: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE history SET is_sing = 1, is_top = 0, times = times + 1, update_time = now() WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn update_name(&self, id: i64, name: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE history SET name = $1, update_time = now() WHERE id = $2")
            .bind(name)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM history WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_for_song(&self, song_id: i64) -> Result<Vec<HistoryRow>, sqlx::Error> {
        sqlx::query_as::<_, HistoryRow>("SELECT * FROM history WHERE id = $1")
            .bind(song_id)
            .fetch_all(&self.pool)
            .await
    }

    /// 待播 + 正在唱，按业务规则排序（唱中置顶 > 手动置顶 > 先点先播）。
    pub async fn list_pending(&self) -> Result<Vec<HistoryRow>, sqlx::Error> {
        let rows = sqlx::query_as::<_, HistoryRow>("SELECT * FROM history WHERE is_sing = ANY($1)")
            .bind(&[QueueState::Singing.to_db(), QueueState::Pending.to_db()][..])
            .fetch_all(&self.pool)
            .await?;

        let mut sort_items: Vec<QueueSortItem> = rows
            .iter()
            .map(|h| QueueSortItem {
                id: h.id,
                is_sing: h.is_sing,
                is_top: h.is_top,
                update_time: h.update_time,
            })
            .collect();
        sort_pending(&mut sort_items);

        let mut by_id: std::collections::HashMap<i64, HistoryRow> =
            rows.into_iter().map(|h| (h.id, h)).collect();
        Ok(sort_items
            .into_iter()
            .filter_map(|item| by_id.remove(&item.id))
            .collect())
    }

    pub async fn list_history_page(
        &self,
        page: i64,
    ) -> Result<(Vec<HistoryRow>, i64), sqlx::Error> {
        let page = page.max(1);
        let offset = (page - 1) * PAGE_SIZE;
        let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM history WHERE is_sing = $1")
            .bind(QueueState::Sung.to_db())
            .fetch_one(&self.pool)
            .await?;
        let rows = sqlx::query_as::<_, HistoryRow>(
            "SELECT * FROM history WHERE is_sing = $1 ORDER BY update_time DESC OFFSET $2 LIMIT $3",
        )
        .bind(QueueState::Sung.to_db())
        .bind(offset)
        .bind(PAGE_SIZE)
        .fetch_all(&self.pool)
        .await?;
        Ok((rows, total))
    }

    pub async fn list_usually_page(
        &self,
        page: i64,
    ) -> Result<(Vec<HistoryRow>, i64), sqlx::Error> {
        let page = page.max(1);
        let offset = (page - 1) * PAGE_SIZE;
        let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM history")
            .fetch_one(&self.pool)
            .await?;
        let rows = sqlx::query_as::<_, HistoryRow>(
            "SELECT * FROM history ORDER BY times DESC OFFSET $1 LIMIT $2",
        )
        .bind(offset)
        .bind(PAGE_SIZE)
        .fetch_all(&self.pool)
        .await?;
        Ok((rows, total))
    }

    /// 启动时把仍标记为"正在唱"的记录复位为已唱（对应服务重启后的状态清理）。
    pub async fn reset_stale_singing(&self) -> Result<u64, sqlx::Error> {
        let result =
            sqlx::query("UPDATE history SET is_sing = 1, update_time = now() WHERE is_sing = -1")
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected())
    }
}
