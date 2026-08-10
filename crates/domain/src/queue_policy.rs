//! 点歌队列状态与排序。对应 Python `karaoke/domain/queue_policy.py`。

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueState {
    Pending,
    Sung,
    Singing,
}

impl QueueState {
    pub fn from_db(value: i32) -> Self {
        match value {
            1 => QueueState::Sung,
            -1 => QueueState::Singing,
            _ => QueueState::Pending,
        }
    }

    pub fn to_db(&self) -> i32 {
        match self {
            QueueState::Pending => 0,
            QueueState::Sung => 1,
            QueueState::Singing => -1,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            QueueState::Pending => "pending",
            QueueState::Sung => "sung",
            QueueState::Singing => "playing",
        }
    }
}

pub fn queue_state_label(is_sing: i32) -> &'static str {
    QueueState::from_db(is_sing).label()
}

pub fn is_playing(is_sing: i32) -> bool {
    QueueState::from_db(is_sing) == QueueState::Singing
}

/// 待播队列排序所需的最小事实集合。
#[derive(Debug, Clone)]
pub struct QueueSortItem {
    pub id: i64,
    pub is_sing: i32,
    pub is_top: i32,
    pub update_time: DateTime<Utc>,
}

/// 排序规则：正在唱的排最前；其余按“置顶优先、置顶内按更新时间倒序”，
/// 非置顶按更新时间正序（先点先播）。与 Python `sort_pending` 等价。
pub fn sort_pending(items: &mut [QueueSortItem]) {
    items.sort_by(|a, b| {
        let key = |item: &QueueSortItem| -> (i32, i64) {
            if QueueState::from_db(item.is_sing) == QueueState::Singing {
                (0, 0)
            } else if item.is_top == 1 {
                (1, -item.update_time.timestamp_millis())
            } else {
                (2, item.update_time.timestamp_millis())
            }
        };
        key(a).cmp(&key(b))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn item(id: i64, is_sing: i32, is_top: i32, minute: u32) -> QueueSortItem {
        QueueSortItem {
            id,
            is_sing,
            is_top,
            update_time: Utc.with_ymd_and_hms(2026, 1, 1, 0, minute, 0).unwrap(),
        }
    }

    #[test]
    fn singing_song_always_first() {
        let mut items = vec![item(1, 0, 0, 5), item(2, -1, 0, 1), item(3, 0, 1, 3)];
        sort_pending(&mut items);
        assert_eq!(items[0].id, 2);
    }

    #[test]
    fn top_songs_sorted_before_pending_by_recency() {
        let mut items = vec![item(1, 0, 0, 1), item(2, 0, 1, 2), item(3, 0, 1, 5)];
        sort_pending(&mut items);
        // 置顶的先出现，且置顶内更新时间更近（5）排在更新时间更早（2）之前
        assert_eq!(items[0].id, 3);
        assert_eq!(items[1].id, 2);
        assert_eq!(items[2].id, 1);
    }

    #[test]
    fn pending_songs_sorted_fifo() {
        let mut items = vec![item(1, 0, 0, 5), item(2, 0, 0, 1)];
        sort_pending(&mut items);
        assert_eq!(items[0].id, 2);
        assert_eq!(items[1].id, 1);
    }

    #[test]
    fn queue_state_label_matches_python_mapping() {
        assert_eq!(queue_state_label(0), "pending");
        assert_eq!(queue_state_label(1), "sung");
        assert_eq!(queue_state_label(-1), "playing");
        assert!(is_playing(-1));
        assert!(!is_playing(1));
    }
}
